pub mod config;
pub mod handshake;
pub mod strategies;

pub mod tungstenite;

pub mod tokio_tungstenite {
    pub use tokio_tungstenite::*;
}

pub mod errors;
pub(crate) mod event_listeners;
pub mod extension;
pub(crate) mod maybe_sender;
pub mod metrics;

/// Simplified type aliases for WebSocket stream components.
pub(crate) mod types {
    use futures_util::stream::{SplitSink, SplitStream};
    use tokio::net::TcpStream;
    use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
    use tungstenite::Message;

    /// WebSocket stream over TCP with optional TLS
    pub type WsTcpStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
    /// WebSocket message sender
    pub type PSTSender = SplitSink<WsTcpStream, Message>;
    /// WebSocket message receiver
    pub type PSTReceiver = SplitStream<WsTcpStream>;
}

/// A prelude module for convenient imports of commonly used items.
pub mod prelude {
    pub use super::config::*;
    pub use super::errors::*;
    pub use super::event_listeners::*;
    pub use super::extension::*;
    pub use super::handshake::*;
    pub use super::maybe_sender::*;
    pub use super::metrics::*;
    pub use super::strategies::*;
    pub use super::tungstenite::*;
    pub use super::types::*;
}
