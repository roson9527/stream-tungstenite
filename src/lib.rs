pub mod config;
pub mod handshake;
pub mod strategies;
pub mod tungstenite;

pub mod errors;
pub(crate) mod event_listeners;
pub(crate) mod extension;
pub(crate) mod maybe_sender;

pub(crate) mod alias {
    use futures_util::stream::{SplitSink, SplitStream};
    use tokio::net::TcpStream;
    use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
    use tungstenite::Message;

    pub type WsTcpStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
    pub type PSTSender = SplitSink<WsTcpStream, Message>;
    pub type PSTReceiver = SplitStream<WsTcpStream>;
}

pub(crate) mod status {
    #[derive(Clone)]
    pub enum WsStreamStatus {
        Connected,
        Disconnected,
    }
}

pub mod prelude {
    pub use super::alias::*;
    pub use super::config::*;
    pub use super::errors::*;
    pub use super::event_listeners::*;
    pub use super::extension::prelude::*;
    pub use super::handshake::*;
    pub use super::maybe_sender::*;
    pub use super::status::*;
    pub use super::strategies::*;
    pub use super::tungstenite::*;
}
