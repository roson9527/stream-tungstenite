use crate::extension::ReconnectTStatusExtension;
use crate::prelude::WsStreamStatus;
use async_trait::async_trait;
use eyre::Result as EResult;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::StreamExt;

pub struct StatusViewer {
    current: Arc<Mutex<WsStreamStatus>>,
}

impl StatusViewer {
    pub fn new() -> Self {
        Self {
            current: Arc::new(Mutex::new(WsStreamStatus::Disconnected)),
        }
    }

    pub async fn current_status(&self) -> WsStreamStatus {
        let status = self.current.lock().await;
        status.clone()
    }
}

#[async_trait]
impl ReconnectTStatusExtension for StatusViewer {
    async fn handle_status_stream(
        &self,
        status_stream: UnboundedReceiverStream<WsStreamStatus>,
    ) -> EResult<()> {
        let status = self.current.clone();
        let stream = status_stream;
        tokio::spawn(async move {
            let status_clone = status.clone();
            let mut stream_clone = stream;
            loop {
                let status = stream_clone.next().await;
                match status {
                    Some(status) => {
                        let mut sc = status_clone.lock().await;
                        *sc = status;
                    }
                    None => {
                        break;
                    }
                }
            }
        });
        Ok(())
    }
}
