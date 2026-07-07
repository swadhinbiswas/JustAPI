use bytes::{Buf, BufMut};
use http::{Request, Response};
use hyper::body::Incoming;
use std::future::Future;
use std::pin::Pin;
use tonic::codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder};
use tonic::server::{Grpc, UnaryService};
use tonic::Status;
use tower::Service;

#[derive(Default, Clone)]
pub struct RawBytesCodec;

impl Codec for RawBytesCodec {
    type Encode = Vec<u8>;
    type Decode = Vec<u8>;
    type Encoder = RawBytesEncoder;
    type Decoder = RawBytesDecoder;

    fn encoder(&mut self) -> Self::Encoder {
        RawBytesEncoder
    }
    fn decoder(&mut self) -> Self::Decoder {
        RawBytesDecoder
    }
}

#[derive(Default, Clone)]
pub struct RawBytesEncoder;

impl Encoder for RawBytesEncoder {
    type Item = Vec<u8>;
    type Error = Status;

    fn encode(&mut self, item: Self::Item, dst: &mut EncodeBuf<'_>) -> Result<(), Self::Error> {
        dst.put_slice(&item);
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct RawBytesDecoder;

impl Decoder for RawBytesDecoder {
    type Item = Vec<u8>;
    type Error = Status;

    fn decode(&mut self, src: &mut DecodeBuf<'_>) -> Result<Option<Self::Item>, Self::Error> {
        if src.has_remaining() {
            let chunk = src.chunk().to_vec();
            src.advance(chunk.len());
            Ok(Some(chunk))
        } else {
            Ok(None)
        }
    }
}

pub type GrpcHandler = Box<
    dyn Fn(
            http::Uri,
            Vec<u8>,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, Status>> + Send + 'static>>
        + Send
        + Sync
        + 'static,
>;

#[derive(Clone)]
pub struct DynamicGrpcService {
    handler: std::sync::Arc<GrpcHandler>,
}

impl DynamicGrpcService {
    pub fn new(handler: GrpcHandler) -> Self {
        Self {
            handler: std::sync::Arc::new(handler),
        }
    }
}

impl Service<Request<Incoming>> for DynamicGrpcService {
    type Response = Response<tonic::body::Body>;
    type Error = std::convert::Infallible;
    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Incoming>) -> Self::Future {
        let handler = self.handler.clone();
        let uri = req.uri().clone();

        Box::pin(async move {
            struct TempSvc {
                uri: http::Uri,
                handler: std::sync::Arc<GrpcHandler>,
            }

            impl UnaryService<Vec<u8>> for TempSvc {
                type Response = Vec<u8>;
                type Future = Pin<
                    Box<
                        dyn Future<Output = Result<tonic::Response<Self::Response>, tonic::Status>>
                            + Send
                            + 'static,
                    >,
                >;

                fn call(&mut self, request: tonic::Request<Vec<u8>>) -> Self::Future {
                    let uri = self.uri.clone();
                    let handler = self.handler.clone();
                    Box::pin(async move {
                        let res = (handler)(uri, request.into_inner()).await?;
                        Ok(tonic::Response::new(res))
                    })
                }
            }

            let mut grpc = Grpc::new(RawBytesCodec);

            // UnaryService expects Request<Incoming> and returns Response<BoxBody>.
            // Since we need to return Response<tonic::body::Body>, we can just map it!
            let res = grpc.unary(TempSvc { uri, handler }, req).await;

            // In tonic 0.14, grpc.unary() returns http::Response<tonic::body::BoxBody> ?
            // Wait, I should just map the body:
            let (parts, body) = res.into_parts();
            let body = tonic::body::Body::new(body); // or similar

            Ok(Response::from_parts(parts, body))
        })
    }
}
