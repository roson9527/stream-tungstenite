use crate::alias::PSTSender;
use crate::errors::ReconnectTError;
use crate::event_listeners::EventListeners;
use eyre::Result as EResult;
use futures_util::SinkExt;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_stream::wrappers::UnboundedReceiverStream;
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
    pub async fn set_sender(&self, sender: PSTSender) {
        let mut lock = self.inner.lock().await;
        *lock = OptPSTSender::Some(sender);
    }

    pub async fn clear_sender(&self) {
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

pub struct ShareListener<T> {
    inner: Arc<Mutex<EventListeners<T>>>,
}

impl<T: Clone> Default for ShareListener<T> {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(EventListeners::default())),
        }
    }
}

impl<T: Clone> ShareListener<T> {
    pub async fn notify(&self, event: T) {
        let mut lock = self.inner.lock().await;
        lock.notify(event);
    }

    pub async fn new_listener(&self) -> UnboundedReceiverStream<T> {
        let mut lock = self.inner.lock().await;
        lock.new_listener()
    }
}
