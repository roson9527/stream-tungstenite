use crate::errors::ReconnectTError;
use crate::types::PSTSender;
use eyre::Result as EResult;
use futures_util::SinkExt;
use std::sync::Arc;
use tokio::sync::Mutex;
use tungstenite::Message;

pub enum OptPSTSender {
    Some(PSTSender),
    None,
}

pub struct MaybePSTSender {
    inner: Arc<Mutex<OptPSTSender>>,
}

impl Default for MaybePSTSender {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(OptPSTSender::None)),
        }
    }
}

impl MaybePSTSender {
    pub(crate) async fn set_sender(&self, sender: PSTSender) {
        let mut lock = self.inner.lock().await;
        *lock = OptPSTSender::Some(sender);
    }

    pub(crate) async fn reset_sender(&self) {
        let mut lock = self.inner.lock().await;
        *lock = OptPSTSender::None;
    }

    pub async fn send(&self, msg: Message) -> EResult<(), ReconnectTError> {
        let mut lock = self.inner.lock().await;
        lock.send(msg).await
    }
}

impl OptPSTSender {
    pub async fn send(&mut self, msg: Message) -> EResult<(), ReconnectTError> {
        match self {
            OptPSTSender::Some(sender) => sender
                .send(msg)
                .await
                .map_err(|e| ReconnectTError::TokioTungsteniteError(e)),
            OptPSTSender::None => Err(ReconnectTError::SenderNotConnected),
        }
    }
}
