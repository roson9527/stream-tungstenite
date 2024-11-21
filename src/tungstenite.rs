use crate::config::ReconnectOptions;
use crate::errors::ReconnectTError;
use crate::maybe_sender::MaybePSTSender;
use crate::prelude::{ExtensionType, ShareListener, WsStreamStatus};
use crate::types::{PSTReceiver, PSTSender, WsTcpStream};
use eyre::Result as EResult;
use futures_util::StreamExt;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::time::{interval_at, Instant};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{client_async_tls_with_config, Connector, MaybeTlsStream, WebSocketStream};
use tungstenite::client::IntoClientRequest;
use tungstenite::error::Error as WsError;
use tungstenite::error::UrlError;
use tungstenite::handshake::client::{Request, Response};
use tungstenite::protocol::WebSocketConfig;
use tungstenite::Error;

pub struct ReconnectT<R> {
    pub request: Box<R>,
    pub option: ReconnectOptions,
    pub sender: Arc<MaybePSTSender>,
    receive_stream: Arc<ShareListener<Message>>,
    status_stream: Arc<ShareListener<WsStreamStatus>>,
}

impl<R: IntoClientRequest + Send + Sync> ReconnectT<R> {
    pub fn new(request: R, option: Option<ReconnectOptions>) -> Self {
        let option = option.unwrap_or_default();
        Self {
            request: Box::new(request),
            option,
            sender: Arc::new(MaybePSTSender::default()),
            receive_stream: Arc::new(ShareListener::default()),
            status_stream: Arc::new(ShareListener::default()),
        }
    }

    pub async fn create_receive_stream(&self) -> UnboundedReceiverStream<Message> {
        self.receive_stream.new_listener().await
    }

    pub async fn create_status_stream(&self) -> UnboundedReceiverStream<WsStreamStatus> {
        self.status_stream.new_listener().await
    }

    pub async fn register_extension(&self, extension: ExtensionType) -> EResult<()> {
        match extension {
            ExtensionType::Msg(extension) => {
                extension
                    .handle_message_stream(self.create_receive_stream().await)
                    .await
            }
            ExtensionType::Status(extension) => {
                extension
                    .handle_status_stream(self.create_status_stream().await)
                    .await
            }
            ExtensionType::All(extension) => {
                extension
                    .handle_message_stream(self.create_receive_stream().await)
                    .await?;
                extension
                    .handle_status_stream(self.create_status_stream().await)
                    .await
            }
        }
    }
}

impl<R: IntoClientRequest + Send + Sync + Clone> ReconnectT<R> {
    pub(crate) async fn connect(&self) -> WsTcpStream {
        let mut retries_to_attempt = self.option.retries_to_attempt_fn()();
        let mut count = 0;
        let request = self
            .request
            .clone()
            .into_client_request()
            .expect("into_client_request");
        loop {
            match connect(request.clone(), None, false, None).await {
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
        writer: &mut PSTSender,
        reader: &mut PSTReceiver,
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
                biased;

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
            self.sender.reset_sender().await;
            let ws_stream = self.connect().await;
            let (mut sender, mut receiver) = ws_stream.split(); // Removed `mut` from receiver
            {
                // handshake
                if let Err(_) = self.handshake(&mut sender, &mut receiver).await {
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

pub trait ArcReconnectTExt {
    fn spawn_run(&self);
}

impl<R: IntoClientRequest + Send + Sync + Clone + 'static> ArcReconnectTExt for Arc<ReconnectT<R>> {
    fn spawn_run(&self) {
        let self_clone = self.clone();
        tokio::spawn(async move { self_clone.run().await });
    }
}

async fn connect(
    request: Request,
    config: Option<WebSocketConfig>,
    disable_nagle: bool,
    connector: Option<Connector>,
) -> Result<(WebSocketStream<MaybeTlsStream<TcpStream>>, Response), Error> {
    let domain = domain(&request)?;
    let port = request
        .uri()
        .port_u16()
        .or_else(|| match request.uri().scheme_str() {
            Some("wss") => Some(443),
            Some("ws") => Some(80),
            _ => None,
        })
        .ok_or(Error::Url(UrlError::UnsupportedUrlScheme))?;

    let addr = format!("{domain}:{port}");
    let socket = TcpStream::connect(addr).await.map_err(Error::Io)?;

    if disable_nagle {
        socket.set_nodelay(true)?;
    }

    client_async_tls_with_config(request, socket, config, connector).await
}

fn domain(request: &Request) -> Result<String, WsError> {
    match request.uri().host() {
        Some(d) => Ok(d.to_string()),
        None => Err(WsError::Url(UrlError::NoHostName)),
    }
}
