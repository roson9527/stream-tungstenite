use crate::extension::interface::ReconnectTExtension;
use crate::prelude::{ShareListener, WsStreamStatus};
use async_trait::async_trait;
use eyre::Result as EResult;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_stream::StreamExt;
use tungstenite::Message;

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
impl ReconnectTExtension for StatusViewer {
    async fn init_msg_stream(&self, _msg_listener: Arc<ShareListener<Message>>) -> EResult<()> {
        // nothing to do
        Ok(())
    }

    async fn init_status_stream(
        &self,
        status_listener: Arc<ShareListener<WsStreamStatus>>,
    ) -> EResult<()> {
        let status = self.current.clone();
        let stream = status_listener.new_listener().await;
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
