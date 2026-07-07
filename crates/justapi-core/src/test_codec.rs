use bytes::{Buf, BufMut};
use tonic::codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder};
use tonic::Status;

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
        let chunk = src.chunk().to_vec();
        src.advance(chunk.len());
        Ok(Some(chunk))
    }
}
