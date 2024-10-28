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

/// Contains type aliases for WebSocket stream components.
pub(crate) mod types {
    use futures_util::stream::{SplitSink, SplitStream};
    use tokio::net::TcpStream;
    use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
    use tungstenite::Message;

    pub type WsTcpStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
    pub type PSTSender = SplitSink<WsTcpStream, Message>;
    pub type PSTReceiver = SplitStream<WsTcpStream>;
}

/// Represents the connection status of a WebSocket stream.
pub(crate) mod status {
    #[derive(Clone)]
    pub enum WsStreamStatus {
        /// The WebSocket stream is connected.
        Connected,
        /// The WebSocket stream is disconnected.
        Disconnected,
    }
}

/// A prelude module for convenient imports of commonly used items.
pub mod prelude {
    pub use super::config::*;
    pub use super::errors::*;
    pub use super::event_listeners::*;
    pub use super::extension::*;
    pub use super::handshake::*;
    pub use super::maybe_sender::*;
    pub use super::status::*;
    pub use super::strategies::*;
    pub use super::tungstenite::*;
    pub use super::types::*;
}
