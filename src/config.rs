use crate::handshake::{NonHandshake, StreamHandshake};
use crate::strategies::{DurationIterator, ExpBackoffStrategy};
use std::sync::Arc;
use std::time::Duration;

/// Configuration options for WebSocket reconnection behavior
#[derive(Clone)]
pub struct ReconnectOptions {
    pub(crate) retries_to_attempt_fn: Arc<dyn Fn() -> DurationIterator + Send + Sync>,
    pub(crate) exit_if_first_connect_fails: bool,
    pub(crate) receive_timeout: Duration,
    pub(crate) handshake: Arc<dyn StreamHandshake + Send + Sync>,
}

impl Default for ReconnectOptions {
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

impl ReconnectOptions {
    /// Create a new ReconnectOptions with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Preset configuration for fast reconnection scenarios
    /// - Short initial delay (500ms)
    /// - Aggressive retry strategy
    /// - Reduced timeout (10s)
    pub fn fast_reconnect() -> Self {
        Self {
            retries_to_attempt_fn: Arc::new(move || {
                Box::new(
                    ExpBackoffStrategy::new(
                        Duration::from_millis(500),
                        1.5,
                        0.1
                    )
                    .with_max(Duration::from_secs(10))
                    .into_iter()
                )
            }),
            exit_if_first_connect_fails: false,
            receive_timeout: Duration::from_secs(10),
            handshake: Arc::new(NonHandshake),
        }
    }

    /// Preset configuration for stable, long-term connections
    /// - Longer initial delay (2s)
    /// - Conservative retry strategy
    /// - Extended timeout (60s)
    pub fn stable_connection() -> Self {
        Self {
            retries_to_attempt_fn: Arc::new(move || {
                Box::new(
                    ExpBackoffStrategy::new(
                        Duration::from_secs(2),
                        2.0,
                        0.05
                    )
                    .with_max(Duration::from_secs(120))
                    .into_iter()
                )
            }),
            exit_if_first_connect_fails: false,
            receive_timeout: Duration::from_secs(60),
            handshake: Arc::new(NonHandshake),
        }
    }

    /// Preset configuration for low-latency, high-frequency scenarios
    /// - Very short delays
    /// - Quick timeout detection
    /// - Minimal backoff
    pub fn low_latency() -> Self {
        Self {
            retries_to_attempt_fn: Arc::new(move || {
                Box::new(
                    ExpBackoffStrategy::new(
                        Duration::from_millis(100),
                        1.2,
                        0.02
                    )
                    .with_max(Duration::from_secs(5))
                    .into_iter()
                )
            }),
            exit_if_first_connect_fails: false,
            receive_timeout: Duration::from_secs(5),
            handshake: Arc::new(NonHandshake),
        }
    }

    // Simplified accessor methods
    pub(crate) fn retries_to_attempt_fn(&self) -> &Arc<dyn Fn() -> DurationIterator + Send + Sync> {
        &self.retries_to_attempt_fn
    }

    pub(crate) fn exit_if_first_connect_fails(&self) -> bool {
        self.exit_if_first_connect_fails
    }

    pub(crate) fn receive_timeout(&self) -> Duration {
        self.receive_timeout
    }

    pub(crate) fn handshake(&self) -> &Arc<dyn StreamHandshake + Send + Sync> {
        &self.handshake
    }
}

// Simplified builder pattern methods
impl ReconnectOptions {
    /// Set a custom handshake implementation
    pub fn with_handshake(mut self, handshake: Arc<dyn StreamHandshake + Send + Sync>) -> Self {
        self.handshake = handshake;
        self
    }

    /// Set the receive timeout duration
    pub fn with_receive_timeout(mut self, receive_timeout: Duration) -> Self {
        self.receive_timeout = receive_timeout;
        self
    }
    
    /// Set a custom retry strategy function
    pub fn with_retry_strategy<F>(mut self, strategy_fn: F) -> Self
    where
        F: Fn() -> DurationIterator + Send + Sync + 'static,
    {
        self.retries_to_attempt_fn = Arc::new(strategy_fn);
        self
    }
    
    /// Set an exponential backoff strategy
    pub fn with_exp_backoff_strategy(mut self, strategy: ExpBackoffStrategy) -> Self {
        self.retries_to_attempt_fn = Arc::new(move || {
            Box::new(strategy.clone().into_iter())
        });
        self
    }

    /// Set whether to exit if the first connection attempt fails
    pub fn with_exit_on_first_failure(mut self, exit_if_first_connect_fails: bool) -> Self {
        self.exit_if_first_connect_fails = exit_if_first_connect_fails;
        self
    }
}

// Deprecated methods for backward compatibility
impl ReconnectOptions {
    /// Build the configuration (now a no-op, kept for compatibility)
    #[deprecated(since = "0.2.0", note = "No longer needed, configuration is ready to use")]
    pub fn build(&mut self) -> Self {
        self.clone()
    }
}
