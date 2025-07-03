use crate::config::ReconnectOptions;
use crate::errors::ReconnectTError;
use crate::maybe_sender::MaybePSTSender;
use crate::prelude::{Extension, ExtensionRegistry, ExtensionEvent, ShareListener, ConnectionStatus};
use crate::types::{PSTReceiver, PSTSender, WsTcpStream};
use eyre::Result as EResult;
use futures_util::StreamExt;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::time::{interval_at, Instant, Duration};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{client_async_tls_with_config, MaybeTlsStream, WebSocketStream};
use tungstenite::client::IntoClientRequest;
use tungstenite::error::Error as WsError;
use tungstenite::error::UrlError;
use tungstenite::handshake::client::{Request, Response};
use tungstenite::protocol::WebSocketConfig;
use tungstenite::Error;
use std::sync::atomic::{AtomicU64, AtomicU8, AtomicBool, Ordering};
use tokio::sync::RwLock;

/// Simplified connection state management
#[derive(Debug)]
pub struct ConnectionState {
    // Basic connection information
    pub connection_id: AtomicU64,
    pub status: AtomicU8, // 0 = Disconnected, 1 = Connected
    pub reconnect_count: AtomicU64,
    pub error_count: AtomicU64,
    
    // Timing information merged into main structure
    pub(crate) connected_at: Arc<RwLock<Option<Instant>>>,
    pub(crate) last_activity: Arc<RwLock<Option<Instant>>>,
    pub(crate) last_error: Arc<RwLock<Option<ReconnectTError>>>,
}

impl ConnectionState {
    fn new(connection_id: u64) -> Self {
        Self {
            connection_id: AtomicU64::new(connection_id),
            status: AtomicU8::new(0), // Disconnected
            reconnect_count: AtomicU64::new(0),
            error_count: AtomicU64::new(0),
            connected_at: Arc::new(RwLock::new(None)),
            last_activity: Arc::new(RwLock::new(None)),
            last_error: Arc::new(RwLock::new(None)),
        }
    }

    /// Fast status check without locking
    pub fn is_connected(&self) -> bool {
        self.status.load(Ordering::Acquire) == 1
    }

    /// Get current status as enum
    pub fn get_status(&self) -> ConnectionStatus {
        match self.status.load(Ordering::Acquire) {
            1 => ConnectionStatus::Connected,
            _ => ConnectionStatus::Disconnected,
        }
    }

    /// Mark as connected
    pub async fn mark_connected(&self) {
        self.status.store(1, Ordering::Release);
        let now = Instant::now();
        
        let mut connected_at = self.connected_at.write().await;
        *connected_at = Some(now);
        
        let mut last_activity = self.last_activity.write().await;
        *last_activity = Some(now);
    }

    /// Mark as disconnected
    pub async fn mark_disconnected(&self) {
        self.status.store(0, Ordering::Release);
        
        let mut connected_at = self.connected_at.write().await;
        *connected_at = None;
    }

    /// Update activity timestamp
    pub async fn update_activity(&self) {
        let mut last_activity = self.last_activity.write().await;
        *last_activity = Some(Instant::now());
    }

    /// Record error with atomic counter
    pub async fn record_error(&self, error: ReconnectTError) {
        self.error_count.fetch_add(1, Ordering::Relaxed);
        let mut last_error = self.last_error.write().await;
        *last_error = Some(error);
    }

    /// Increment reconnect counter
    pub fn increment_reconnect(&self) {
        self.reconnect_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Check if connection is healthy based on activity
    pub async fn is_healthy(&self, timeout: Duration) -> bool {
        if !self.is_connected() {
            return false;
        }
        
        let last_activity = self.last_activity.read().await;
        if let Some(activity_time) = *last_activity {
            activity_time.elapsed() < timeout
        } else {
            false
        }
    }

    /// Get a snapshot of the current state for external use
    pub async fn get_snapshot(&self) -> ConnectionStateSnapshot {
        let connected_at = *self.connected_at.read().await;
        let last_activity = *self.last_activity.read().await;
        let last_error = self.last_error.read().await;
        
        ConnectionStateSnapshot {
            connection_id: self.connection_id.load(Ordering::Relaxed),
            status: self.get_status(),
            connected_at,
            last_activity,
            reconnect_count: self.reconnect_count.load(Ordering::Relaxed),
            error_count: self.error_count.load(Ordering::Relaxed),
            last_error: last_error.clone(),
        }
    }
}

/// Snapshot of connection state for external consumption
#[derive(Debug, Clone)]
pub struct ConnectionStateSnapshot {
    pub connection_id: u64,
    pub status: ConnectionStatus,
    pub connected_at: Option<Instant>,
    pub last_activity: Option<Instant>,
    pub reconnect_count: u64,
    pub error_count: u64,
    pub last_error: Option<ReconnectTError>,
}

/// Internal connection manager for optimized connection handling
struct ConnectionManager<R> {
    request: Box<R>,
    options: ReconnectOptions,
    state: Arc<ConnectionState>,
    connection_id_counter: Arc<AtomicU64>,
    is_shutting_down: Arc<AtomicBool>,
}

impl<R: IntoClientRequest + Send + Sync + Clone> ConnectionManager<R> {
    fn new(request: R, options: ReconnectOptions) -> Self {
        let connection_id = 1;
        Self {
            request: Box::new(request),
            options,
            state: Arc::new(ConnectionState::new(connection_id)),
            connection_id_counter: Arc::new(AtomicU64::new(connection_id)),
            is_shutting_down: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Get current connection state snapshot
    async fn get_state_snapshot(&self) -> ConnectionStateSnapshot {
        self.state.get_snapshot().await
    }

    /// Check if connected without locking
    fn is_connected(&self) -> bool {
        self.state.is_connected()
    }

    /// Intelligent connection attempt with error-based retry strategy
    async fn connect_with_retry(&self) -> Result<WsTcpStream, ReconnectTError> {
        let mut retries_to_attempt = self.options.retries_to_attempt_fn()();
        let mut retry_count = 0;
        
        loop {
            if self.is_shutting_down.load(Ordering::Relaxed) {
                return Err(ReconnectTError::connection_failed("Connection attempt cancelled"));
            }

            let connection_id = self.connection_id_counter.fetch_add(1, Ordering::Relaxed);
            
            let request = self.request
                .clone()
                .into_client_request()
                .map_err(|e| ReconnectTError::config_error(format!("Invalid request: {}", e)))?;

            match Self::try_connect(request, None, false).await {
                Ok((ws_stream, _)) => {
                    // Update connection state
                    self.state.connection_id.store(connection_id, Ordering::Relaxed);
                    self.state.mark_connected().await;
                    
                    if retry_count > 0 {
                        self.state.increment_reconnect();
                        tracing::info!(
                            connection_id = connection_id,
                            retry_count = retry_count,
                            "Successfully reconnected"
                        );
                    }
                    
                    return Ok(ws_stream);
                }
                Err(e) => {
                    let reconnect_error = ReconnectTError::connection_failed(format!("Connection failed: {}", e));
                    
                    self.state.record_error(reconnect_error.clone()).await;

                    // Determine if we should retry based on error severity
                    if !reconnect_error.is_retryable() {
                        tracing::error!(
                            connection_id = connection_id,
                            error = ?e,
                            "Fatal connection error, stopping retry attempts"
                        );
                        return Err(reconnect_error);
                    }

                    retry_count += 1;
                    
                    // Get retry delay based on error type
                    let delay = reconnect_error.recommended_retry_delay()
                        .unwrap_or_else(|| {
                            retries_to_attempt.next()
                                .unwrap_or(Duration::from_secs(60))
                        });

                    tracing::warn!(
                        connection_id = connection_id,
                        retry_count = retry_count,
                        error = ?e,
                        next_retry_in = ?delay,
                        error_severity = ?reconnect_error.severity(),
                        "Connection failed, retrying with intelligent delay"
                    );

                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    /// Enhanced connection attempt with better error handling
    async fn try_connect(
        request: Request,
        config: Option<WebSocketConfig>,
        disable_nagle: bool,
    ) -> Result<(WebSocketStream<MaybeTlsStream<TcpStream>>, Response), Error> {
        let domain = Self::extract_domain(&request)?;
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
        
        let socket = TcpStream::connect(&addr)
            .await
            .map_err(|e| {
                tracing::debug!(
                    addr = %addr,
                    error = ?e,
                    "TCP connection failed"
                );
                Error::Io(e)
            })?;

        if disable_nagle {
            socket.set_nodelay(true)?;
        }

        client_async_tls_with_config(request, socket, config, None).await
    }

    fn extract_domain(request: &Request) -> Result<String, WsError> {
        request
            .uri()
            .host()
            .map(|d| d.to_string())
            .ok_or(WsError::Url(UrlError::NoHostName))
    }

    /// Shutdown the connection manager
    fn shutdown(&self) {
        self.is_shutting_down.store(true, Ordering::Relaxed);
    }
}

pub struct ReconnectT<R> {
    pub request: Box<R>,
    pub option: ReconnectOptions,
    pub sender: Arc<MaybePSTSender>,
    receive_stream: Arc<ShareListener<Message>>,
    status_stream: Arc<ShareListener<ConnectionStatus>>,
    connection_manager: Arc<ConnectionManager<R>>,
    extension_registry: Arc<ExtensionRegistry>,
}

impl<R: IntoClientRequest + Send + Sync + Clone> ReconnectT<R> {
    pub fn new(request: R, option: Option<ReconnectOptions>) -> Self {
        let option = option.unwrap_or_default();
        let connection_manager = Arc::new(ConnectionManager::new(request.clone(), option.clone()));
        let extension_registry = Arc::new(ExtensionRegistry::new(1));
        
        Self {
            request: Box::new(request),
            option,
            sender: Arc::new(MaybePSTSender::default()),
            receive_stream: Arc::new(ShareListener::default()),
            status_stream: Arc::new(ShareListener::default()),
            connection_manager,
            extension_registry,
        }
    }

    pub async fn create_receive_stream(&self) -> UnboundedReceiverStream<Message> {
        self.receive_stream.new_listener().await
    }

    pub async fn create_status_stream(&self) -> UnboundedReceiverStream<ConnectionStatus> {
        self.status_stream.new_listener().await
    }

    pub async fn register_extension<E>(&self, extension: E) -> EResult<()> 
    where
        E: Extension + 'static,
    {
        // Using the new unified extension interface
        tracing::info!(
            extension = extension.name(),
            version = extension.version(),
            description = extension.description(),
            capabilities = ?extension.capabilities(),
            "Registering extension"
        );
        
        // Register the extension with the extension registry
        self.extension_registry.register(extension).await?;
        
        Ok(())
    }

    /// Get current connection state for monitoring
    pub async fn get_connection_state(&self) -> ConnectionStateSnapshot {
        self.connection_manager.get_state_snapshot().await
    }

    /// Check if connection is healthy
    pub fn is_connection_healthy(&self) -> bool {
        self.connection_manager.is_connected()
    }
}

impl<R: IntoClientRequest + Send + Sync + Clone> ReconnectT<R> {
    // Updated to use the new connection manager
    pub(crate) async fn connect(&self) -> WsTcpStream {
        match self.connection_manager.connect_with_retry().await {
            Ok(stream) => stream,
            Err(e) => {
                tracing::error!(error = ?e, "Failed to establish connection after retries");
                // For backward compatibility, we'll keep trying even if connection manager fails
                // This maintains the original behavior of infinite retry
                self.legacy_connect().await
            }
        }
    }

    /// Legacy connection method for backward compatibility
    async fn legacy_connect(&self) -> WsTcpStream {
        let mut retries_to_attempt = self.option.retries_to_attempt_fn()();
        let mut count = 0;
        let request = self
            .request
            .clone()
            .into_client_request()
            .expect("Failed to convert request");
        
        loop {
            match Self::try_connect(request.clone(), None, false).await {
                Ok((ws_stream, _)) => {
                    if count > 0 {
                        tracing::info!(count=count, "Successfully reconnected");
                    }
                    return ws_stream;
                }
                Err(e) => {
                    count += 1;
                    let delay = retries_to_attempt
                        .next()
                        .expect("Retry iterator should be infinite");
                    
                    tracing::warn!(
                        count=count, 
                        error=?e, 
                        next_retry_in=?delay,
                        "Connection failed, retrying"
                    );
                    
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    async fn try_connect(
        request: Request,
        config: Option<WebSocketConfig>,
        disable_nagle: bool,
    ) -> Result<(WebSocketStream<MaybeTlsStream<TcpStream>>, Response), Error> {
        let domain = Self::extract_domain(&request)?;
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
        let socket = TcpStream::connect(&addr)
            .await
            .map_err(|e| {
                tracing::debug!(addr=%addr, error=?e, "TCP connection failed");
                Error::Io(e)
            })?;

        if disable_nagle {
            socket.set_nodelay(true)?;
        }

        client_async_tls_with_config(request, socket, config, None).await
    }

    fn extract_domain(request: &Request) -> Result<String, WsError> {
        request
            .uri()
            .host()
            .map(|d| d.to_string())
            .ok_or(WsError::Url(UrlError::NoHostName))
    }

    pub(crate) async fn handshake(
        &self,
        writer: &mut PSTSender,
        reader: &mut PSTReceiver,
    ) -> EResult<(), ReconnectTError> {
        let result = self.option
            .handshake()
            .handshake(writer, reader)
            .await
            .map_err(|e| {
                tracing::error!(error=?e, "Handshake failed");
                ReconnectTError::protocol_error(format!("Handshake failed: {}", e))
            });

        // Update connection state based on handshake result
        match &result {
            Ok(_) => {
                self.connection_manager.state.update_activity().await;
            }
            Err(e) => {
                self.connection_manager.state.record_error(e.clone()).await;
            }
        }

        result
    }

    pub(crate) async fn receive_loop(
        &self,
        mut receiver: PSTReceiver,
    ) -> EResult<(), ReconnectTError> {
        let receive_timeout = self.option.receive_timeout();
        let listener = self.receive_stream.clone();
        
        // 使用 interval_at 创建定时器，从当前时间 + timeout 开始
        let start = Instant::now() + receive_timeout;
        let mut timeout_interval = interval_at(start, receive_timeout);
        timeout_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                biased;

                msg = receiver.next() => {
                    match msg {
                        Some(Ok(msg)) => {
                            // Update connection activity
                            self.connection_manager.state.update_activity().await;
                            
                            // Notify extensions about received message
                            if let Err(e) = self.extension_registry.notify_message(&msg).await {
                                tracing::warn!(error=?e, "Extension message notification failed");
                            }
                            
                            listener.notify(msg).await;
                            // 重置超时计时器
                            timeout_interval.reset();
                        },
                        Some(Err(e)) => {
                            tracing::error!(error=?e, "Error receiving message");
                            let error = ReconnectTError::TokioTungsteniteError(e);
                            
                            // Record error in connection state
                            self.connection_manager.state.record_error(error.clone()).await;
                            
                            return Err(error);
                        },
                        None => {
                            tracing::debug!("WebSocket stream closed");
                            break;
                        }
                    }
                }
                _ = timeout_interval.tick() => {
                    tracing::warn!(
                        timeout=?receive_timeout,
                        "Receive timeout exceeded"
                    );
                    let error = ReconnectTError::timeout_error("receive", receive_timeout);
                    
                    // Record timeout error
                    self.connection_manager.state.record_error(error.clone()).await;
                    
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    pub async fn run(&self) {
        loop {
                    let state = self.get_connection_state().await;
        let reconnect_count = state.reconnect_count + 1;
        
        if reconnect_count > 1 {
            tracing::info!(
                reconnect_count = reconnect_count,
                connection_id = state.connection_id,
                "Starting reconnection attempt"
            );
            }
            
            self.sender.reset_sender().await;
            
            // Mark as disconnected
            self.connection_manager.state.mark_disconnected().await;
            
            // Establish connection
            let ws_stream = self.connect().await;
            let (mut sender, mut receiver) = ws_stream.split();
            
            // Update extension registry context with new connection info
            let state = self.get_connection_state().await;
            self.extension_registry.update_context(state.connection_id, state.reconnect_count).await;
            
            // Perform handshake
            match self.handshake(&mut sender, &mut receiver).await {
                Ok(_) => {
                    self.sender.set_sender(sender).await;
                    self.status_stream.notify(ConnectionStatus::Connected).await;
                    
                    let state = self.get_connection_state().await;
                    tracing::info!(
                        connection_id = state.connection_id,
                        "WebSocket connected and handshake completed"
                    );
                    
                    // Notify extensions about connection
                    if let Err(e) = self.extension_registry.notify_event(ExtensionEvent::Connected).await {
                        tracing::warn!(error=?e, "Extension connection notification failed");
                    }
                }
                Err(e) => {
                    tracing::error!(error=?e, "Handshake failed, reconnecting");
                    self.status_stream.notify(ConnectionStatus::Disconnected).await;
                    
                    // Notify extensions about disconnection
                    if let Err(ext_err) = self.extension_registry.notify_event(ExtensionEvent::Disconnected).await {
                        tracing::warn!(error=?ext_err, "Extension disconnection notification failed");
                    }
                    
                    // Check if we should apply intelligent retry delay
                    if let Some(delay) = e.recommended_retry_delay() {
                        tracing::info!(delay=?delay, "Applying intelligent retry delay");
                        tokio::time::sleep(delay).await;
                    }
                    
                    continue;
                }
            }

            // Start receive loop
            match self.receive_loop(receiver).await {
                Ok(_) => {
                    tracing::info!("Receive loop ended normally");
                }
                Err(e) => {
                    tracing::error!(error=?e, "Receive loop error");
                    
                    // Apply intelligent retry delay based on error severity
                    if let Some(delay) = e.recommended_retry_delay() {
                        tracing::info!(delay=?delay, "Applying intelligent retry delay");
                        tokio::time::sleep(delay).await;
                    }
                }
            }
            
            self.status_stream.notify(ConnectionStatus::Disconnected).await;
            tracing::info!("WebSocket disconnected, preparing to reconnect");
            
            // Notify extensions about disconnection
            if let Err(e) = self.extension_registry.notify_event(ExtensionEvent::Disconnected).await {
                tracing::warn!(error=?e, "Extension disconnection notification failed");
            }
        }
    }
}

pub trait ArcReconnectTExt {
    fn spawn_run(&self);
    fn spawn_run_with_handle(&self) -> tokio::task::JoinHandle<()>;
}

impl<R: IntoClientRequest + Send + Sync + Clone + 'static> ArcReconnectTExt for Arc<ReconnectT<R>> {
    fn spawn_run(&self) {
        let self_clone = self.clone();
        tokio::spawn(async move { self_clone.run().await });
    }
    
    fn spawn_run_with_handle(&self) -> tokio::task::JoinHandle<()> {
        let self_clone = self.clone();
        tokio::spawn(async move { self_clone.run().await })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ReconnectOptions;
    use crate::strategies::ExpBackoffStrategy;
    use std::time::Duration;

    #[tokio::test]
    async fn test_reconnect_t_creation() {
        let request = "ws://localhost:8080";
        let reconnect = ReconnectT::new(request, None);
        
        assert!(reconnect.option.receive_timeout() == std::time::Duration::from_secs(20));
    }

    #[tokio::test]
    async fn test_with_custom_timeout() {
        let request = "ws://localhost:8080";
        let options = ReconnectOptions::default()
            .with_receive_timeout(std::time::Duration::from_secs(30));
        
        let reconnect = ReconnectT::new(request, Some(options));
        assert_eq!(reconnect.option.receive_timeout(), std::time::Duration::from_secs(30));
    }
    
    #[tokio::test]
    async fn test_with_custom_backoff_strategy() {
        let request = "ws://localhost:8080";
        
        // 使用更激进的退避策略：初始延迟1秒，最大延迟30秒
        let custom_strategy = ExpBackoffStrategy::new(
            Duration::from_secs(1),
            1.5,  // 较小的增长因子
            0.1   // 10% 的抖动
        ).with_max(Duration::from_secs(30));
        
        let options = ReconnectOptions::default()
            .with_exp_backoff_strategy(custom_strategy);
        
        let reconnect = ReconnectT::new(request, Some(options));
        // 验证策略已设置（通过获取第一个重试延迟）
        let mut retry_iter = reconnect.option.retries_to_attempt_fn()();
        let first_delay = retry_iter.next().unwrap();
        assert!(first_delay >= Duration::from_millis(900)); // 1秒 - 10%
        assert!(first_delay <= Duration::from_millis(1100)); // 1秒 + 10%
    }

    #[test]
    fn test_extract_domain() {
        let request = "ws://example.com:8080/path"
            .into_client_request()
            .unwrap();
        
        let domain = ReconnectT::<&str>::extract_domain(&request).unwrap();
        assert_eq!(domain, "example.com");
    }
    
    #[tokio::test]
    async fn test_interval_reset_behavior() {
        use tokio::time::{interval_at, sleep, Duration, Instant};
        
        let start = Instant::now();
        let mut interval = interval_at(start + Duration::from_millis(100), Duration::from_millis(100));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        
        // 第一个 tick 应该在 100ms 后触发
        interval.tick().await;
        let elapsed1 = start.elapsed();
        println!("First tick at: {:?}", elapsed1);
        
        // 等待 50ms（还有 50ms 才到下一个 tick）
        sleep(Duration::from_millis(50)).await;
        let before_reset = start.elapsed();
        println!("Before reset at: {:?}", before_reset);
        
        // 现在 reset，这会取消即将在 50ms 后发生的 tick
        interval.reset();
        let reset_time = Instant::now();
        
        // 下一个 tick 应该在 reset 后的 100ms 触发
        interval.tick().await;
        let after_reset = reset_time.elapsed();
        let total_elapsed = start.elapsed();
        
        println!("After reset tick at: {:?} (from reset: {:?})", total_elapsed, after_reset);
        
        // 验证：从 reset 时刻算起，应该等待了约 100ms
        assert!(after_reset >= Duration::from_millis(95) && after_reset <= Duration::from_millis(110));
        
        // 验证 reset 确实取消了原本的 tick
        // 如果没有 reset，第二个 tick 应该在 200ms 时触发
        // 但因为 reset，实际上是在 150ms + 100ms = 250ms 左右触发
        assert!(total_elapsed >= Duration::from_millis(240));
    }
}
