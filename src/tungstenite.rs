use crate::config::ReconnectOptions;
use crate::errors::ReconnectTError;
use crate::event_listeners::EventListeners;
use eyre::Result as EResult;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::StreamExt;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time::{interval_at, Instant};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

pub type WsTcpStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
pub type PSTSender = SplitSink<WsTcpStream, Message>;
pub type PSTReceiver = SplitStream<WsTcpStream>;

pub struct ReconnectT {
    pub url: String,
    pub option: ReconnectOptions,
    pub sender: Arc<Mutex<Option<PSTSender>>>, // using Mutex to lock the sender,
    pub receiver_stream: Arc<Mutex<EventListeners<Message>>>,
}

impl ReconnectT {
    pub fn new(url: String, option: Option<ReconnectOptions>) -> Self {
        let option = option.unwrap_or_default();
        Self {
            url,
            option,
            sender: Arc::new(Mutex::new(None)),
            receiver_stream: Arc::new(Mutex::new(EventListeners::default())),
        }
    }
}

impl ReconnectT {
    pub async fn connect(&self) -> WsTcpStream {
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

    pub async fn handshake(
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

    pub async fn receive_loop(&self, mut receiver: PSTReceiver) -> EResult<(), ReconnectTError> {
        let receive_timeout = self.option.receive_timeout();
        let start_time = Instant::now();
        let mut receive_timeout_tick = interval_at(start_time + receive_timeout, receive_timeout);

        let listener = self.receiver_stream.clone();
        loop {
            tokio::select! {
                msg = receiver.next() => {
                    match msg {
                        Some(Ok(msg)) => {
                            let mut listener = listener.lock().await;
                            listener.notify(msg);
                            receive_timeout_tick.reset();
                        },
                        Some(Err(e)) => {
                            return Err(ReconnectTError::ReceiveErrMessage(e));
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
            {
                let mut self_sender = self.sender.lock().await; // Declare as mutable
                *self_sender = None;
            }
            let ws_stream = self.connect().await;
            let (sender, receiver) = ws_stream.split(); // Removed `mut` from receiver
            {
                // handshake
                if let Err(_) = self.handshake(&sender, &receiver).await {
                    continue;
                }
                let mut self_sender = self.sender.lock().await; // Declare as mutable
                *self_sender = Some(sender);
            }

            // receive loop
            if let Err(e) = self.receive_loop(receiver).await {
                tracing::error!(error=?e, "reconnect::receive_loop");
            }
        }
    }
}
