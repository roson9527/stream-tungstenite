use crate::types::{PSTReceiver, PSTSender};
use async_trait::async_trait;
use eyre::Result as EResult;

#[async_trait]
pub trait StreamHandshake {
    async fn handshake(&self, writer: &PSTSender, reader: &PSTReceiver) -> EResult<()>;
}

pub struct NonHandshake;

#[async_trait]
impl StreamHandshake for NonHandshake {
    async fn handshake(&self, _writer: &PSTSender, _reader: &PSTReceiver) -> EResult<()> {
        Ok(())
    }
}
