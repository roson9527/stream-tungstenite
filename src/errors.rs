use core::time::Duration;

#[derive(thiserror::Error, Debug)]
pub enum ReconnectTError {
    #[error("receive timeout: {0:?}")]
    ReceiveTimeout(Duration),
    #[error("handshake failed")]
    HandshakeFailed,
    #[error("sender not connected")]
    SenderNotConnected,
    #[error("tokio_tungstenite error: {0}")]
    TokioTungsteniteError(#[from] tokio_tungstenite::tungstenite::Error),
}
