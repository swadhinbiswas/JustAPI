use std::future::Future;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

/// A request wrapped with a oneshot channel to return the result.
pub struct BatchRequest<Req, Res> {
    pub payload: Req,
    pub responder: oneshot::Sender<Res>,
}

/// A handle to push items into the batcher.
#[derive(Clone)]
pub struct Batcher<Req, Res> {
    sender: mpsc::Sender<BatchRequest<Req, Res>>,
}

impl<Req, Res> Batcher<Req, Res>
where
    Req: Send + 'static,
    Res: Send + 'static,
{
    /// Send a request to the batching queue and wait for the response.
    pub async fn execute(&self, payload: Req) -> Result<Res, &'static str> {
        let (tx, rx) = oneshot::channel();
        let req = BatchRequest {
            payload,
            responder: tx,
        };

        if self.sender.send(req).await.is_err() {
            return Err("Batcher is closed");
        }

        rx.await.map_err(|_| "Batcher dropped the response")
    }
}

/// Start an adaptive batching loop.
///
/// `max_size` is the maximum number of items in a single batch.
/// `window` is the maximum time to wait for items before flushing a partial batch.
/// `processor` is an async closure that takes a `Vec<Req>` and returns a `Vec<Res>`.
/// The returned vector must have the exact same length as the input vector.
pub fn start_batcher<Req, Res, F, Fut>(
    max_size: usize,
    window: Duration,
    mut processor: F,
) -> Batcher<Req, Res>
where
    Req: Send + 'static,
    Res: Send + 'static,
    F: FnMut(Vec<Req>) -> Fut + Send + 'static,
    Fut: Future<Output = Vec<Res>> + Send + 'static,
{
    let (tx, mut rx) = mpsc::channel::<BatchRequest<Req, Res>>(1024);

    tokio::spawn(async move {
        loop {
            let mut batch = Vec::with_capacity(max_size);
            let mut responders = Vec::with_capacity(max_size);

            // Wait for the FIRST item in the batch.
            // If the channel is closed, we exit the loop.
            let first_req = match rx.recv().await {
                Some(req) => req,
                None => break,
            };

            batch.push(first_req.payload);
            responders.push(first_req.responder);

            // Now we have the first item, we wait for more up to `max_size` or `window`.
            let deadline = tokio::time::sleep(window);
            tokio::pin!(deadline);

            loop {
                if batch.len() >= max_size {
                    break;
                }

                tokio::select! {
                    _ = &mut deadline => {
                        // Timeout reached, flush what we have.
                        break;
                    }
                    opt_req = rx.recv() => {
                        match opt_req {
                            Some(req) => {
                                batch.push(req.payload);
                                responders.push(req.responder);
                            }
                            None => {
                                // Channel closed, break and flush the remaining batch.
                                break;
                            }
                        }
                    }
                }
            }

            // Process the batch
            let results = processor(batch).await;

            // Scatter the results back to the callers
            for (responder, res) in responders.into_iter().zip(results) {
                let _ = responder.send(res);
            }
        }
    });

    Batcher { sender: tx }
}
