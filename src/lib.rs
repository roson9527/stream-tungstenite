pub mod config;
pub mod handshake;
pub(crate) mod strategies;
pub mod tungstenite;

pub mod errors;
pub(crate) mod event_listeners;
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

pub mod prelude {
    pub use super::alias::*;
    pub use super::config::*;
    pub use super::errors::*;
    pub use super::event_listeners::*;
    pub use super::handshake::*;
    pub use super::strategies::*;
    pub use super::tungstenite::*;
}
