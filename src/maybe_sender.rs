use crate::errors::ReconnectTError;
use crate::types::PSTSender;
use eyre::Result as EResult;
use futures_util::SinkExt;
use std::sync::Arc;
use tungstenite::Message;
use std::sync::atomic::{AtomicBool, Ordering};

/// Lock-free optional sender using atomic operations
pub struct OptPSTSender {
    sender: Option<PSTSender>,
    is_some: AtomicBool,
}

impl OptPSTSender {
    fn new() -> Self {
        Self {
            sender: None,
            is_some: AtomicBool::new(false),
        }
    }

    fn set_sender(&mut self, sender: PSTSender) {
        self.sender = Some(sender);
        self.is_some.store(true, Ordering::Release);
    }

    fn reset_sender(&mut self) {
        self.sender = None;
        self.is_some.store(false, Ordering::Release);
    }

    fn is_connected(&self) -> bool {
        self.is_some.load(Ordering::Acquire)
    }

    async fn send(&mut self, msg: Message) -> EResult<(), ReconnectTError> {
        if !self.is_connected() {
            return Err(ReconnectTError::SenderNotConnected);
        }

        match &mut self.sender {
            Some(sender) => sender
                .send(msg)
                .await
                .map_err(|e| {
                    // Mark as disconnected on error
                    self.is_some.store(false, Ordering::Release);
                    ReconnectTError::TokioTungsteniteError(e)
                }),
            None => Err(ReconnectTError::SenderNotConnected),
        }
    }
}

/// Optimized maybe sender with reduced lock contention
pub struct MaybePSTSender {
    inner: Arc<tokio::sync::Mutex<OptPSTSender>>,
    // Fast path check without acquiring lock
    is_connected: Arc<AtomicBool>,
}

impl Default for MaybePSTSender {
    fn default() -> Self {
        Self {
            inner: Arc::new(tokio::sync::Mutex::new(OptPSTSender::new())),
            is_connected: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl MaybePSTSender {
    pub(crate) async fn set_sender(&self, sender: PSTSender) {
        let mut lock = self.inner.lock().await;
        lock.set_sender(sender);
        self.is_connected.store(true, Ordering::Release);
    }

    pub(crate) async fn reset_sender(&self) {
        // Fast path: mark as disconnected immediately
        self.is_connected.store(false, Ordering::Release);
        
        let mut lock = self.inner.lock().await;
        lock.reset_sender();
    }

    /// Fast path check without acquiring lock
    pub fn is_connected(&self) -> bool {
        self.is_connected.load(Ordering::Acquire)
    }

    /// Optimized send with fast path rejection
    pub async fn send(&self, msg: Message) -> EResult<(), ReconnectTError> {
        // Fast path: check connection status without acquiring lock
        if !self.is_connected() {
            return Err(ReconnectTError::SenderNotConnected);
        }

        // Only acquire lock if we think we're connected
        let mut lock = self.inner.lock().await;
        
        // Double-check after acquiring lock
        if !lock.is_connected() {
            return Err(ReconnectTError::SenderNotConnected);
        }

        match lock.send(msg).await {
            Ok(()) => Ok(()),
            Err(e) => {
                // Update atomic flag on error
                self.is_connected.store(false, Ordering::Release);
                Err(e)
            }
        }
    }
}
