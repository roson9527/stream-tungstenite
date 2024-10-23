use crate::handshake::{NonHandshake, StreamHandshake};
use crate::strategies::{DurationIterator, ExpBackoffStrategy};
use std::sync::Arc;
use std::time::Duration;

pub struct ReconnectOptions {
    inner: Box<Inner>,
}

impl Default for ReconnectOptions {
    fn default() -> Self {
        Self {
            inner: Box::new(Inner::default()),
        }
    }
}

impl ReconnectOptions {
    pub(crate) fn retries_to_attempt_fn(&self) -> &Arc<dyn Fn() -> DurationIterator + Send + Sync> {
        &self.inner.retries_to_attempt_fn
    }

    #[allow(dead_code)]
    pub(crate) fn exit_if_first_connect_fails(&self) -> bool {
        self.inner.exit_if_first_connect_fails
    }

    pub(crate) fn receive_timeout(&self) -> Duration {
        self.inner.receive_timeout
    }

    pub(crate) fn handshake(&self) -> &Arc<dyn StreamHandshake + Send + Sync> {
        &self.inner.handshake
    }
}

impl ReconnectOptions {
    pub fn set_handshake(&mut self, handshake: Arc<dyn StreamHandshake + Send + Sync>) {
        self.inner.handshake = handshake;
    }
}

#[derive(Clone)]
struct Inner {
    retries_to_attempt_fn: Arc<dyn Fn() -> DurationIterator + Send + Sync>,
    exit_if_first_connect_fails: bool,
    receive_timeout: Duration,
    handshake: Arc<dyn StreamHandshake + Send + Sync>,
}

impl Default for Inner {
    fn default() -> Self {
        Self {
            retries_to_attempt_fn: Arc::new(move || {
                Box::new(ExpBackoffStrategy::default().into_iter())
            }),
            exit_if_first_connect_fails: false,
            receive_timeout: Duration::from_secs(20),
            handshake: Arc::new(NonHandshake),
        }
    }
}
