use crate::extension::{Extension, ExtensionContext, ExtensionCapabilities};
use async_trait::async_trait;
use eyre::Result as EResult;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Represents the connection status of a WebSocket stream
#[derive(Clone, Debug)]
pub enum ConnectionStatus {
    /// The WebSocket stream is connected
    Connected,
    /// The WebSocket stream is disconnected
    Disconnected,
}

/// Status viewer extension that tracks current WebSocket connection status
pub struct StatusViewer {
    current: Arc<Mutex<ConnectionStatus>>,
}

impl StatusViewer {
    pub fn new() -> Self {
        Self {
            current: Arc::new(Mutex::new(ConnectionStatus::Disconnected)),
        }
    }

    pub async fn current_status(&self) -> ConnectionStatus {
        let status = self.current.lock().await;
        status.clone()
    }

    /// Get a shared reference to the current status
    pub fn status_handle(&self) -> Arc<Mutex<ConnectionStatus>> {
        self.current.clone()
    }
}

impl Default for StatusViewer {
    fn default() -> Self {
        Self::new()
    }
}

// Unified extension interface implementation
#[async_trait]
impl Extension for StatusViewer {
    fn name(&self) -> &'static str {
        "StatusViewer"
    }

    fn version(&self) -> &'static str {
        "2.0.0"
    }

    fn description(&self) -> &'static str {
        "Tracks current WebSocket connection status and provides real-time access"
    }

    fn capabilities(&self) -> ExtensionCapabilities {
        ExtensionCapabilities::new().with_lifecycle()
    }

    async fn on_initialize(&self, context: &ExtensionContext) -> EResult<()> {
        tracing::debug!(
            extension = self.name(),
            connection_id = context.connection_id,
            "StatusViewer extension initialized"
        );
        Ok(())
    }

    async fn on_connected(&self, context: &ExtensionContext) -> EResult<()> {
        tracing::info!(
            extension = self.name(),
            connection_id = context.connection_id,
            reconnect_count = context.reconnect_count,
            "Connection established"
        );
        
        let mut status = self.current.lock().await;
        *status = ConnectionStatus::Connected;
        Ok(())
    }

    async fn on_disconnected(&self, context: &ExtensionContext) -> EResult<()> {
        tracing::info!(
            extension = self.name(),
            connection_id = context.connection_id,
            "Connection lost"
        );
        
        let mut status = self.current.lock().await;
        *status = ConnectionStatus::Disconnected;
        Ok(())
    }

    async fn on_shutdown(&self, context: &ExtensionContext) -> EResult<()> {
        tracing::debug!(
            extension = self.name(),
            connection_id = context.connection_id,
            "StatusViewer extension shutting down"
        );
        Ok(())
    }
}

/// Enhanced status viewer with additional features
pub struct AdvancedStatusViewer {
    status_viewer: StatusViewer,
    connection_history: Arc<Mutex<Vec<(std::time::Instant, ConnectionStatus)>>>,
    max_history: usize,
}

impl AdvancedStatusViewer {
    pub fn new() -> Self {
        Self::with_history_limit(100)
    }

    pub fn with_history_limit(max_history: usize) -> Self {
        Self {
            status_viewer: StatusViewer::new(),
            connection_history: Arc::new(Mutex::new(Vec::new())),
            max_history,
        }
    }

    pub async fn current_status(&self) -> ConnectionStatus {
        self.status_viewer.current_status().await
    }

    pub async fn get_history(&self) -> Vec<(std::time::Instant, ConnectionStatus)> {
        let history = self.connection_history.lock().await;
        history.clone()
    }

    pub async fn get_uptime(&self) -> Option<std::time::Duration> {
        let history = self.connection_history.lock().await;
        
        // Find the most recent connection event
        for (time, status) in history.iter().rev() {
            if matches!(status, ConnectionStatus::Connected) {
                return Some(time.elapsed());
            }
        }
        
        None
    }

    pub async fn get_connection_count(&self) -> usize {
        let history = self.connection_history.lock().await;
        history.iter()
            .filter(|(_, status)| matches!(status, ConnectionStatus::Connected))
            .count()
    }

    async fn add_to_history(&self, status: ConnectionStatus) {
        let mut history = self.connection_history.lock().await;
        
        let entry = (std::time::Instant::now(), status);
        history.push(entry);
        
        // Keep only the most recent entries
        if history.len() > self.max_history {
            history.remove(0);
        }
    }
}

impl Default for AdvancedStatusViewer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Extension for AdvancedStatusViewer {
    fn name(&self) -> &'static str {
        "AdvancedStatusViewer"
    }

    fn version(&self) -> &'static str {
        "2.0.0"
    }

    fn description(&self) -> &'static str {
        "Advanced status viewer with connection history and metrics"
    }

    fn capabilities(&self) -> ExtensionCapabilities {
        ExtensionCapabilities::new().with_lifecycle()
    }

    async fn on_initialize(&self, context: &ExtensionContext) -> EResult<()> {
        tracing::debug!(
            extension = self.name(),
            connection_id = context.connection_id,
            "AdvancedStatusViewer extension initialized"
        );
        Ok(())
    }

    async fn on_connected(&self, context: &ExtensionContext) -> EResult<()> {
        tracing::info!(
            extension = self.name(),
            connection_id = context.connection_id,
            reconnect_count = context.reconnect_count,
            "Connection established"
        );
        
        self.add_to_history(ConnectionStatus::Connected).await;
        Ok(())
    }

    async fn on_disconnected(&self, context: &ExtensionContext) -> EResult<()> {
        tracing::info!(
            extension = self.name(),
            connection_id = context.connection_id,
            "Connection lost"
        );
        
        self.add_to_history(ConnectionStatus::Disconnected).await;
        Ok(())
    }

    async fn on_shutdown(&self, context: &ExtensionContext) -> EResult<()> {
        tracing::debug!(
            extension = self.name(),
            connection_id = context.connection_id,
            "AdvancedStatusViewer extension shutting down"
        );
        Ok(())
    }
}
