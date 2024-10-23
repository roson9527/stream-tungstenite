use core::time::Duration;

#[derive(thiserror::Error, Debug)]
pub enum ReconnectTError {
    #[error("receive error message: {0}")]
    ReceiveErrMessage(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("receive timeout: {0:?}")]
    ReceiveTimeout(Duration),
    #[error("handshake failed")]
    HandshakeFailed,
}
