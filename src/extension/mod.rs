mod status_viewer;

pub mod interface {
    use async_trait::async_trait;
    use eyre::Result as EResult;
    use std::sync::Arc;
    use tungstenite::Message;
    use std::collections::HashMap;
    use tokio::sync::RwLock;

    /// Simplified extension events
    #[derive(Debug, Clone)]
    pub enum ExtensionEvent {
        /// Extension is being initialized
        Initialize,
        /// Connection established
        Connected,
        /// Connection lost
        Disconnected,
        /// Extension is being shutdown
        Shutdown,
    }

    /// Extension context providing access to connection information
    #[derive(Debug, Clone)]
    pub struct ExtensionContext {
        pub connection_id: u64,
        pub reconnect_count: u64,
        pub metadata: HashMap<String, String>,
    }

    impl ExtensionContext {
        pub fn new(connection_id: u64) -> Self {
            Self {
                connection_id,
                reconnect_count: 0,
                metadata: HashMap::new(),
            }
        }

        pub fn with_metadata<K, V>(mut self, key: K, value: V) -> Self
        where
            K: Into<String>,
            V: Into<String>,
        {
            self.metadata.insert(key.into(), value.into());
            self
        }

        pub fn get_metadata(&self, key: &str) -> Option<&String> {
            self.metadata.get(key)
        }
    }

    /// Simplified extension trait
    #[async_trait]
    pub trait Extension: Send + Sync {
        /// Extension name for identification
        fn name(&self) -> &'static str;

        /// Extension version
        fn version(&self) -> &'static str {
            "1.0.0"
        }

        /// Extension description
        fn description(&self) -> &'static str {
            ""
        }

        /// Extension capabilities - what this extension can handle
        fn capabilities(&self) -> ExtensionCapabilities {
            ExtensionCapabilities::default()
        }

        /// Called when extension lifecycle events occur
        async fn on_event(&self, event: ExtensionEvent, context: &ExtensionContext) -> EResult<()> {
            match event {
                ExtensionEvent::Initialize => self.on_initialize(context).await,
                ExtensionEvent::Connected => self.on_connected(context).await,
                ExtensionEvent::Disconnected => self.on_disconnected(context).await,
                ExtensionEvent::Shutdown => self.on_shutdown(context).await,
            }
        }

        /// Called when extension is initialized (default implementation)
        async fn on_initialize(&self, _context: &ExtensionContext) -> EResult<()> {
            Ok(())
        }

        /// Called when connection is established (default implementation)
        async fn on_connected(&self, _context: &ExtensionContext) -> EResult<()> {
            Ok(())
        }

        /// Called when connection is lost (default implementation)
        async fn on_disconnected(&self, _context: &ExtensionContext) -> EResult<()> {
            Ok(())
        }

        /// Called when extension is being shutdown (default implementation)
        async fn on_shutdown(&self, _context: &ExtensionContext) -> EResult<()> {
            Ok(())
        }

        /// Handle incoming messages (default implementation)
        async fn on_message(&self, _message: &Message, _context: &ExtensionContext) -> EResult<()> {
            Ok(())
        }
    }

    /// Extension capabilities to indicate what an extension can handle
    #[derive(Debug, Clone, Default)]
    pub struct ExtensionCapabilities {
        pub handles_messages: bool,
        pub handles_lifecycle: bool,
    }

    impl ExtensionCapabilities {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn with_messages(mut self) -> Self {
            self.handles_messages = true;
            self
        }

        pub fn with_lifecycle(mut self) -> Self {
            self.handles_lifecycle = true;
            self
        }

        pub fn with_all(mut self) -> Self {
            self.handles_messages = true;
            self.handles_lifecycle = true;
            self
        }
    }

    /// Simplified extension registry
    pub struct ExtensionRegistry {
        extensions: Arc<RwLock<Vec<Arc<dyn Extension>>>>,
        context: Arc<tokio::sync::Mutex<ExtensionContext>>,
    }

    impl ExtensionRegistry {
        pub fn new(connection_id: u64) -> Self {
            Self {
                extensions: Arc::new(RwLock::new(Vec::new())),
                context: Arc::new(tokio::sync::Mutex::new(ExtensionContext::new(connection_id))),
            }
        }

        /// Register a new extension
        pub async fn register<E>(&self, extension: E) -> EResult<()>
        where
            E: Extension + 'static,
        {
            let extension = Arc::new(extension);
            
            // Initialize the extension
            let context = self.context.lock().await;
            extension.on_event(ExtensionEvent::Initialize, &*context).await?;
            drop(context);
            
            let mut extensions = self.extensions.write().await;
            extensions.push(extension);
            
            Ok(())
        }

        /// Get all registered extensions
        pub async fn get_extensions(&self) -> Vec<Arc<dyn Extension>> {
            let extensions = self.extensions.read().await;
            extensions.clone()
        }

        /// Get extensions that handle messages
        pub async fn get_message_extensions(&self) -> Vec<Arc<dyn Extension>> {
            let extensions = self.extensions.read().await;
            extensions.iter()
                .filter(|ext| ext.capabilities().handles_messages)
                .cloned()
                .collect()
        }

        /// Get extensions that handle lifecycle events
        pub async fn get_lifecycle_extensions(&self) -> Vec<Arc<dyn Extension>> {
            let extensions = self.extensions.read().await;
            extensions.iter()
                .filter(|ext| ext.capabilities().handles_lifecycle)
                .cloned()
                .collect()
        }

        /// Notify all extensions of an event
        pub async fn notify_event(&self, event: ExtensionEvent) -> EResult<()> {
            let extensions = self.get_lifecycle_extensions().await;
            let context = self.context.lock().await;
            
            for extension in extensions {
                if let Err(e) = extension.on_event(event.clone(), &*context).await {
                    tracing::warn!(
                        extension = extension.name(),
                        event = ?event,
                        error = ?e,
                        "Extension event handler failed"
                    );
                }
            }
            
            Ok(())
        }

        /// Notify message extensions
        pub async fn notify_message(&self, message: &Message) -> EResult<()> {
            let extensions = self.get_message_extensions().await;
            let context = self.context.lock().await;
            
            for extension in extensions {
                if let Err(e) = extension.on_message(message, &*context).await {
                    tracing::warn!(
                        extension = extension.name(),
                        error = ?e,
                        "Extension message handler failed"
                    );
                }
            }
            
            Ok(())
        }

        /// Update context (e.g., when reconnecting)
        pub async fn update_context(&self, connection_id: u64, reconnect_count: u64) {
            let mut context = self.context.lock().await;
            context.connection_id = connection_id;
            context.reconnect_count = reconnect_count;
        }

        /// Shutdown all extensions
        pub async fn shutdown(&self) -> EResult<()> {
            self.notify_event(ExtensionEvent::Shutdown).await
        }
    }

    /// Simplified message extension using closures
    pub struct MessageExtension<F> {
        name: &'static str,
        version: &'static str,
        description: &'static str,
        capabilities: ExtensionCapabilities,
        handler: F,
    }

    impl<F> MessageExtension<F> {
        pub fn new(name: &'static str, handler: F) -> Self {
            Self {
                name,
                version: "1.0.0",
                description: "",
                capabilities: ExtensionCapabilities::new().with_messages(),
                handler,
            }
        }

        pub fn with_version(mut self, version: &'static str) -> Self {
            self.version = version;
            self
        }

        pub fn with_description(mut self, description: &'static str) -> Self {
            self.description = description;
            self
        }
    }

    #[async_trait]
    impl<F> Extension for MessageExtension<F>
    where
        F: Fn(&Message, &ExtensionContext) -> EResult<()> + Send + Sync,
    {
        fn name(&self) -> &'static str {
            self.name
        }

        fn version(&self) -> &'static str {
            self.version
        }

        fn description(&self) -> &'static str {
            self.description
        }

        fn capabilities(&self) -> ExtensionCapabilities {
            self.capabilities.clone()
        }

        async fn on_message(&self, message: &Message, context: &ExtensionContext) -> EResult<()> {
            (self.handler)(message, context)
        }
    }

    /// Convenience functions for creating extensions
    pub fn message_extension<F>(name: &'static str, handler: F) -> MessageExtension<F>
    where
        F: Fn(&Message, &ExtensionContext) -> EResult<()> + Send + Sync + 'static,
    {
        MessageExtension::new(name, handler)
    }
}

pub use crate::extension::interface::*;
pub use crate::extension::status_viewer::*;
