use crate::types::{PSTReceiver, PSTSender};
use async_trait::async_trait;
use eyre::Result as EResult;
use futures_util::SinkExt;
use tokio_stream::StreamExt;
use tungstenite::Message;

#[async_trait]
pub trait StreamHandshake {
    async fn handshake(&self, writer: &mut PSTSender, reader: &mut PSTReceiver) -> EResult<()>;
}

pub struct NonHandshake;

#[async_trait]
impl StreamHandshake for NonHandshake {
    async fn handshake(&self, _writer: &mut PSTSender, _reader: &mut PSTReceiver) -> EResult<()> {
        Ok(())
    }
}

pub struct SingleHandshake;

#[async_trait]
impl StreamHandshake for SingleHandshake {
    async fn handshake(&self, writer: &mut PSTSender, reader: &mut PSTReceiver) -> EResult<()> {
        let _ = writer
            .send(Message::Text("hello world".to_string()))
            .await?;
        let _ = reader.next().await;
        Ok(())
    }
}
