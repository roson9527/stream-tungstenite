use core::time::Duration;
use std::time::Instant;

/// Error context providing additional information about when and where an error occurred
#[derive(Debug, Clone)]
pub struct ErrorContext {
    pub timestamp: Instant,
    pub retry_count: u32,
    pub connection_id: Option<u64>,
    pub additional_info: Option<String>,
}

impl ErrorContext {
    pub fn new() -> Self {
        Self {
            timestamp: Instant::now(),
            retry_count: 0,
            connection_id: None,
            additional_info: None,
        }
    }

    pub fn with_retry_count(mut self, retry_count: u32) -> Self {
        self.retry_count = retry_count;
        self
    }

    pub fn with_connection_id(mut self, connection_id: u64) -> Self {
        self.connection_id = Some(connection_id);
        self
    }

    pub fn with_info<S: Into<String>>(mut self, info: S) -> Self {
        self.additional_info = Some(info.into());
        self
    }
}

/// Severity level of errors for different handling strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    /// Recoverable errors that can be retried immediately
    Warning,
    /// Errors that require retry with backoff
    Error,
    /// Critical errors that may require connection reset
    Critical,
    /// Fatal errors that should stop reconnection attempts
    Fatal,
}

/// Simplified error types for WebSocket reconnection operations
#[derive(thiserror::Error, Debug)]
pub enum ReconnectTError {
    /// Connection-related errors
    #[error("Connection failed: {message}")]
    ConnectionError {
        message: String,
        context: ErrorContext,
    },

    /// Protocol-related errors
    #[error("Protocol error: {message}")]
    ProtocolError {
        message: String,
        context: ErrorContext,
    },

    /// Configuration errors
    #[error("Configuration error: {message}")]
    ConfigurationError {
        message: String,
        context: ErrorContext,
    },

    /// Timeout errors with specific timeout information
    #[error("Timeout error: {timeout_type} timeout of {duration:?} exceeded")]
    TimeoutError {
        timeout_type: String,
        duration: Duration,
        context: ErrorContext,
    },

    /// Authentication/authorization errors
    #[error("Authentication failed: {message}")]
    AuthenticationError {
        message: String,
        context: ErrorContext,
    },

    /// Resource exhaustion errors
    #[error("Resource exhausted: {resource_type}")]
    ResourceExhausted {
        resource_type: String,
        context: ErrorContext,
    },

    /// Legacy errors for backward compatibility
    #[error("receive timeout: {0:?}")]
    ReceiveTimeout(Duration),

    #[error("handshake failed")]
    HandshakeFailed,

    #[error("sender not connected")]
    SenderNotConnected,

    #[error("tokio_tungstenite error: {0}")]
    TokioTungsteniteError(#[from] tokio_tungstenite::tungstenite::Error),
}

impl ReconnectTError {
    /// Get the severity level of the error - simplified logic
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            ReconnectTError::ConnectionError { .. } => ErrorSeverity::Error,
            ReconnectTError::ProtocolError { .. } => ErrorSeverity::Critical,
            ReconnectTError::ConfigurationError { .. } => ErrorSeverity::Fatal,
            ReconnectTError::TimeoutError { .. } => ErrorSeverity::Error,
            ReconnectTError::AuthenticationError { .. } => ErrorSeverity::Critical,
            ReconnectTError::ResourceExhausted { .. } => ErrorSeverity::Critical,
            // Legacy errors
            ReconnectTError::ReceiveTimeout(_) => ErrorSeverity::Error,
            ReconnectTError::HandshakeFailed => ErrorSeverity::Error,
            ReconnectTError::SenderNotConnected => ErrorSeverity::Warning,
            ReconnectTError::TokioTungsteniteError(e) => {
                // Simplified classification
                match e {
                    tokio_tungstenite::tungstenite::Error::ConnectionClosed => ErrorSeverity::Warning,
                    tokio_tungstenite::tungstenite::Error::AlreadyClosed => ErrorSeverity::Warning,
                    tokio_tungstenite::tungstenite::Error::Url(_) => ErrorSeverity::Fatal,
                    tokio_tungstenite::tungstenite::Error::HttpFormat(_) => ErrorSeverity::Fatal,
                    tokio_tungstenite::tungstenite::Error::Protocol(_) => ErrorSeverity::Critical,
                    _ => ErrorSeverity::Error,
                }
            }
        }
    }

    /// Get the error context if available
    pub fn context(&self) -> Option<&ErrorContext> {
        match self {
            ReconnectTError::ConnectionError { context, .. } => Some(context),
            ReconnectTError::ProtocolError { context, .. } => Some(context),
            ReconnectTError::ConfigurationError { context, .. } => Some(context),
            ReconnectTError::TimeoutError { context, .. } => Some(context),
            ReconnectTError::AuthenticationError { context, .. } => Some(context),
            ReconnectTError::ResourceExhausted { context, .. } => Some(context),
            _ => None,
        }
    }

    /// Check if this error is retryable based on its severity
    pub fn is_retryable(&self) -> bool {
        match self.severity() {
            ErrorSeverity::Warning | ErrorSeverity::Error => true,
            ErrorSeverity::Critical => true, // Critical errors may be retryable after connection reset
            ErrorSeverity::Fatal => false,
        }
    }

    /// Check if this error requires a connection reset
    pub fn requires_connection_reset(&self) -> bool {
        match self.severity() {
            ErrorSeverity::Critical => true,
            _ => false,
        }
    }

    /// Get the recommended retry delay based on error type and context
    pub fn recommended_retry_delay(&self) -> Option<Duration> {
        match self.severity() {
            ErrorSeverity::Warning => Some(Duration::from_millis(100)),
            ErrorSeverity::Error => Some(Duration::from_secs(1)),
            ErrorSeverity::Critical => Some(Duration::from_secs(5)),
            ErrorSeverity::Fatal => None,
        }
    }
}

// Convenience constructors for common error patterns
impl ReconnectTError {
    pub fn connection_failed<S: Into<String>>(message: S) -> Self {
        Self::ConnectionError {
            message: message.into(),
            context: ErrorContext::new(),
        }
    }

    pub fn protocol_error<S: Into<String>>(message: S) -> Self {
        Self::ProtocolError {
            message: message.into(),
            context: ErrorContext::new(),
        }
    }

    pub fn timeout_error<S: Into<String>>(timeout_type: S, duration: Duration) -> Self {
        Self::TimeoutError {
            timeout_type: timeout_type.into(),
            duration,
            context: ErrorContext::new(),
        }
    }

    pub fn auth_error<S: Into<String>>(message: S) -> Self {
        Self::AuthenticationError {
            message: message.into(),
            context: ErrorContext::new(),
        }
    }

    pub fn config_error<S: Into<String>>(message: S) -> Self {
        Self::ConfigurationError {
            message: message.into(),
            context: ErrorContext::new(),
        }
    }

    pub fn resource_exhausted<S: Into<String>>(resource_type: S) -> Self {
        Self::ResourceExhausted {
            resource_type: resource_type.into(),
            context: ErrorContext::new(),
        }
    }
}

// Legacy error constructors for backward compatibility
impl ReconnectTError {
    /// Create a legacy receive timeout error
    pub fn receive_timeout(duration: Duration) -> Self {
        Self::ReceiveTimeout(duration)
    }

    /// Create a legacy handshake failed error
    pub fn handshake_failed() -> Self {
        Self::HandshakeFailed
    }

    /// Create a legacy sender not connected error
    pub fn sender_not_connected() -> Self {
        Self::SenderNotConnected
    }
}

// Manual Clone implementation since tungstenite::Error doesn't implement Clone
impl Clone for ReconnectTError {
    fn clone(&self) -> Self {
        match self {
            ReconnectTError::ConnectionError { message, context } => {
                ReconnectTError::ConnectionError {
                    message: message.clone(),
                    context: context.clone(),
                }
            }
            ReconnectTError::ProtocolError { message, context } => {
                ReconnectTError::ProtocolError {
                    message: message.clone(),
                    context: context.clone(),
                }
            }
            ReconnectTError::ConfigurationError { message, context } => {
                ReconnectTError::ConfigurationError {
                    message: message.clone(),
                    context: context.clone(),
                }
            }
            ReconnectTError::TimeoutError { timeout_type, duration, context } => {
                ReconnectTError::TimeoutError {
                    timeout_type: timeout_type.clone(),
                    duration: *duration,
                    context: context.clone(),
                }
            }
            ReconnectTError::AuthenticationError { message, context } => {
                ReconnectTError::AuthenticationError {
                    message: message.clone(),
                    context: context.clone(),
                }
            }
            ReconnectTError::ResourceExhausted { resource_type, context } => {
                ReconnectTError::ResourceExhausted {
                    resource_type: resource_type.clone(),
                    context: context.clone(),
                }
            }
            ReconnectTError::ReceiveTimeout(duration) => {
                ReconnectTError::ReceiveTimeout(*duration)
            }
            ReconnectTError::HandshakeFailed => {
                ReconnectTError::HandshakeFailed
            }
            ReconnectTError::SenderNotConnected => {
                ReconnectTError::SenderNotConnected
            }
            ReconnectTError::TokioTungsteniteError(e) => {
                // Since tungstenite::Error doesn't implement Clone, we'll convert to string and back
                // This is not ideal but necessary for compatibility
                ReconnectTError::connection_failed(format!("Tungstenite error: {}", e))
            }
        }
    }
}
