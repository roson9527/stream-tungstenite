use crate::alias::{PSTReceiver, PSTSender, WsTcpStream};
use crate::config::ReconnectOptions;
use crate::errors::ReconnectTError;
use crate::maybe_sender::{MaybePSTSender, ShareListener};
use eyre::Result as EResult;
use futures_util::StreamExt;
use std::sync::Arc;
use tokio::time::{interval_at, Instant};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

#[derive(Clone)]

pub enum WsStreamStatus {
    Connected,
    Disconnected,
}

pub struct ReconnectT {
    pub url: String,
    pub option: ReconnectOptions,
    pub sender: Arc<MaybePSTSender>,
    receive_stream: Arc<ShareListener<Message>>,
    status_stream: Arc<ShareListener<WsStreamStatus>>,
}

impl ReconnectT {
    pub fn new(url: String, option: Option<ReconnectOptions>) -> Self {
        let option = option.unwrap_or_default();
        Self {
            url,
            option,
            sender: Arc::new(MaybePSTSender::default()),
            receive_stream: Arc::new(ShareListener::default()),
            status_stream: Arc::new(ShareListener::default()),
        }
    }

    pub async fn new_receive_stream(&self) -> UnboundedReceiverStream<Message> {
        self.receive_stream.new_listener().await
    }

    pub async fn new_status_stream(&self) -> UnboundedReceiverStream<WsStreamStatus> {
        self.status_stream.new_listener().await
    }
}

impl ReconnectT {
    pub(crate) async fn connect(&self) -> WsTcpStream {
        let mut retries_to_attempt = self.option.retries_to_attempt_fn()();
        let mut count = 0;
        loop {
            match connect_async(self.url.clone()).await {
                Ok((ws_stream, _)) => return ws_stream,
                Err(e) => {
                    count += 1;
                    tracing::warn!(count=count, error=?e, "reconnect::connect");
                    tokio::time::sleep(
                        retries_to_attempt
                            .next()
                            .expect("retries_to_attempt_fn::next() should not return None"),
                    )
                    .await;
                }
            }
        }
    }

    pub(crate) async fn handshake(
        &self,
        writer: &PSTSender,
        reader: &PSTReceiver,
    ) -> EResult<(), ReconnectTError> {
        match self.option.handshake().handshake(writer, reader).await {
            Ok(_) => Ok(()),
            Err(e) => {
                tracing::error!(error=?e, "reconnect::handshake");
                Err(ReconnectTError::HandshakeFailed)
            }
        }
    }

    pub(crate) async fn receive_loop(
        &self,
        mut receiver: PSTReceiver,
    ) -> EResult<(), ReconnectTError> {
        let receive_timeout = self.option.receive_timeout();
        let start_time = Instant::now();
        let mut receive_timeout_tick = interval_at(start_time + receive_timeout, receive_timeout);

        let listener = self.receive_stream.clone();
        loop {
            tokio::select! {
                msg = receiver.next() => {
                    match msg {
                        Some(Ok(msg)) => {
                            listener.notify(msg).await;
                            receive_timeout_tick.reset();
                        },
                        Some(Err(e)) => {
                            return Err(ReconnectTError::TokioTungsteniteError(e));
                        },
                        None => break, // Connection closed
                    }
                }
                _ = receive_timeout_tick.tick() => {
                    return Err(ReconnectTError::ReceiveTimeout(receive_timeout));
                }
            }
        }
        Ok(())
    }

    pub async fn run(&self) {
        loop {
            self.sender.clear_sender().await;
            let ws_stream = self.connect().await;
            let (sender, receiver) = ws_stream.split(); // Removed `mut` from receiver
            {
                // handshake
                if let Err(_) = self.handshake(&sender, &receiver).await {
                    continue;
                }
                self.sender.set_sender(sender).await;
                self.status_stream.notify(WsStreamStatus::Connected).await;
            }

            // receive loop
            if let Err(e) = self.receive_loop(receiver).await {
                tracing::error!(error=?e, "reconnect::receive_loop");
            }
            self.status_stream
                .notify(WsStreamStatus::Disconnected)
                .await;
        }
    }
}
