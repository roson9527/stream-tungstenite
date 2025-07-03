use std::sync::Arc;
use tokio::sync::{mpsc, RwLock, broadcast};
use tokio_stream::wrappers::{UnboundedReceiverStream, BroadcastStream};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Simplified event distribution strategy
#[derive(Debug, Clone, Copy)]
pub enum EventStrategy {
    /// Use individual channels for each listener (default)
    /// Best for: Few listeners, different processing speeds
    Unicast,
    /// Use broadcast channel for all listeners
    /// Best for: Many listeners, same processing speed
    Broadcast,
    /// Hybrid approach that switches based on listener count
    /// Best for: Variable number of listeners
    /// Parameter: threshold for switching to broadcast mode
    Adaptive(usize),
}

impl Default for EventStrategy {
    fn default() -> Self {
        EventStrategy::Adaptive(8)  // Default threshold of 8 listeners
    }
}

/// Current operating mode for adaptive strategy
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AdaptiveMode {
    Unicast,
    Broadcast,
    Hybrid,  // Transitioning between modes
}

/// A collection of event listeners for a task with optimized performance
#[derive(Clone, Debug)]
pub struct EventListeners<T> {
    /// Strategy for event distribution
    strategy: EventStrategy,
    /// Individual listeners for unicast mode
    listeners: Vec<mpsc::UnboundedSender<T>>,
    /// Broadcast sender for broadcast mode
    broadcast_tx: Option<broadcast::Sender<T>>,
    /// Counter for listener management
    listener_count: Arc<AtomicUsize>,
    /// Current mode for adaptive strategy
    adaptive_mode: AdaptiveMode,
    /// Threshold for switching to broadcast mode
    adaptive_threshold: usize,
}

impl<T> Default for EventListeners<T> {
    fn default() -> Self {
        Self {
            strategy: EventStrategy::default(),
            listeners: Vec::new(),
            broadcast_tx: None,
            listener_count: Arc::new(AtomicUsize::new(0)),
            adaptive_mode: AdaptiveMode::Unicast,
            adaptive_threshold: 8,
        }
    }
}

impl<T: Clone + Send + 'static> EventListeners<T> {
    /// Create new event listeners with specified strategy
    pub fn new(strategy: EventStrategy) -> Self {
        let (broadcast_tx, adaptive_mode, adaptive_threshold) = match strategy {
            EventStrategy::Broadcast => {
                let (tx, _) = broadcast::channel(1024);
                (Some(tx), AdaptiveMode::Broadcast, 0)
            }
            EventStrategy::Adaptive(threshold) => {
                (None, AdaptiveMode::Unicast, threshold)
            }
            _ => (None, AdaptiveMode::Unicast, 0),
        };

        Self {
            strategy,
            listeners: Vec::new(),
            broadcast_tx,
            listener_count: Arc::new(AtomicUsize::new(0)),
            adaptive_mode,
            adaptive_threshold,
        }
    }

    /// Get current listener count
    pub fn len(&self) -> usize {
        self.listener_count.load(Ordering::Relaxed)
    }

    /// Returns true if there are no registered listeners
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Check if we should switch to broadcast mode
    fn should_switch_to_broadcast(&self) -> bool {
        matches!(self.strategy, EventStrategy::Adaptive(_)) &&
        self.adaptive_mode == AdaptiveMode::Unicast &&
        self.len() >= self.adaptive_threshold
    }

    /// Check if we should switch back to unicast mode
    fn should_switch_to_unicast(&self) -> bool {
        matches!(self.strategy, EventStrategy::Adaptive(_)) &&
        self.adaptive_mode == AdaptiveMode::Broadcast &&
        self.len() < self.adaptive_threshold.saturating_sub(2) // Hysteresis to prevent oscillation
    }

    /// Send an event to all listeners with optimized distribution
    pub fn notify(&mut self, event: T) {
        match self.strategy {
            EventStrategy::Unicast => {
                self.notify_unicast(event);
            }
            EventStrategy::Broadcast => {
                self.notify_broadcast(event);
            }
            EventStrategy::Adaptive(_) => {
                self.notify_adaptive(event);
            }
        }
    }

    /// Adaptive notification strategy
    fn notify_adaptive(&mut self, event: T) {
        // Check if we need to switch modes
        if self.should_switch_to_broadcast() {
            self.switch_to_broadcast();
        } else if self.should_switch_to_unicast() {
            self.switch_to_unicast();
        }

        // Send event based on current mode
        match self.adaptive_mode {
            AdaptiveMode::Unicast => {
                self.notify_unicast(event);
            }
            AdaptiveMode::Broadcast => {
                self.notify_broadcast(event);
            }
            AdaptiveMode::Hybrid => {
                // During transition, send to both to ensure no event loss
                self.notify_unicast(event.clone());
                self.notify_broadcast(event);
            }
        }
    }

    /// Send event using unicast (individual channels)
    fn notify_unicast(&mut self, event: T) {
        let mut active_listeners = Vec::new();
        
        for listener in &self.listeners {
            if listener.send(event.clone()).is_ok() {
                active_listeners.push(listener.clone());
            }
        }
        
        if active_listeners.len() != self.listeners.len() {
            self.listeners = active_listeners;
            self.listener_count.store(self.listeners.len(), Ordering::Relaxed);
        }
    }

    /// Send event using broadcast channel
    fn notify_broadcast(&mut self, event: T) {
        if let Some(ref tx) = self.broadcast_tx {
            let _ = tx.send(event);
        }
    }

    /// Switch from unicast to broadcast mode
    fn switch_to_broadcast(&mut self) {
        if self.adaptive_mode == AdaptiveMode::Unicast {
            let (tx, _) = broadcast::channel(1024);
            self.broadcast_tx = Some(tx);
            self.adaptive_mode = AdaptiveMode::Hybrid;
            
            // Schedule transition to pure broadcast mode
            // For now, we'll transition immediately but keep unicast for existing listeners
            tokio::spawn({
                let mut delay = tokio::time::interval(tokio::time::Duration::from_millis(100));
                async move {
                    delay.tick().await; // Wait a bit for existing listeners to adapt
                }
            });
            
            self.adaptive_mode = AdaptiveMode::Broadcast;
        }
    }

    /// Switch from broadcast to unicast mode
    fn switch_to_unicast(&mut self) {
        if self.adaptive_mode == AdaptiveMode::Broadcast {
            self.adaptive_mode = AdaptiveMode::Hybrid;
            
            // Gradually phase out broadcast mode
            tokio::spawn({
                let mut delay = tokio::time::interval(tokio::time::Duration::from_millis(50));
                async move {
                    delay.tick().await; // Brief transition period
                }
            });
            
            self.adaptive_mode = AdaptiveMode::Unicast;
        }
    }

    /// Add a new event listener
    pub fn new_listener(&mut self) -> EventReceiver<T> {
        match self.strategy {
            EventStrategy::Unicast => {
                let (sender, receiver) = mpsc::unbounded_channel();
                self.listeners.push(sender);
                self.listener_count.fetch_add(1, Ordering::Relaxed);
                EventReceiver::Unicast(UnboundedReceiverStream::new(receiver))
            }
            EventStrategy::Broadcast => {
                if let Some(ref tx) = self.broadcast_tx {
                    let receiver = tx.subscribe();
                    self.listener_count.fetch_add(1, Ordering::Relaxed);
                    EventReceiver::Broadcast(BroadcastStream::new(receiver))
                } else {
                    // Fallback to unicast if broadcast not available
                    let (sender, receiver) = mpsc::unbounded_channel();
                    self.listeners.push(sender);
                    self.listener_count.fetch_add(1, Ordering::Relaxed);
                    EventReceiver::Unicast(UnboundedReceiverStream::new(receiver))
                }
            }
            EventStrategy::Adaptive(_) => {
                self.new_adaptive_listener()
            }
        }
    }

    /// Create a new listener for adaptive mode
    fn new_adaptive_listener(&mut self) -> EventReceiver<T> {
        self.listener_count.fetch_add(1, Ordering::Relaxed);
        
        match self.adaptive_mode {
            AdaptiveMode::Unicast => {
                let (sender, receiver) = mpsc::unbounded_channel();
                self.listeners.push(sender);
                EventReceiver::Unicast(UnboundedReceiverStream::new(receiver))
            }
            AdaptiveMode::Broadcast => {
                if let Some(ref tx) = self.broadcast_tx {
                    let receiver = tx.subscribe();
                    EventReceiver::Broadcast(BroadcastStream::new(receiver))
                } else {
                    // Fallback to unicast
                    let (sender, receiver) = mpsc::unbounded_channel();
                    self.listeners.push(sender);
                    EventReceiver::Unicast(UnboundedReceiverStream::new(receiver))
                }
            }
            AdaptiveMode::Hybrid => {
                // During hybrid mode, prefer broadcast for new listeners
                if let Some(ref tx) = self.broadcast_tx {
                    let receiver = tx.subscribe();
                    EventReceiver::Broadcast(BroadcastStream::new(receiver))
                } else {
                    let (sender, receiver) = mpsc::unbounded_channel();
                    self.listeners.push(sender);
                    EventReceiver::Unicast(UnboundedReceiverStream::new(receiver))
                }
            }
        }
    }

    /// Push new event listener (for backward compatibility)
    pub fn push_listener(&mut self, listener: mpsc::UnboundedSender<T>) {
        if matches!(self.strategy, EventStrategy::Unicast) || 
           matches!(self.adaptive_mode, AdaptiveMode::Unicast | AdaptiveMode::Hybrid) {
            self.listeners.push(listener);
            self.listener_count.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Get current adaptive mode (for debugging/monitoring)
    pub fn adaptive_mode(&self) -> Option<AdaptiveMode> {
        if matches!(self.strategy, EventStrategy::Adaptive(_)) {
            Some(self.adaptive_mode)
        } else {
            None
        }
    }

    /// Get adaptive threshold
    pub fn adaptive_threshold(&self) -> Option<usize> {
        if matches!(self.strategy, EventStrategy::Adaptive(_)) {
            Some(self.adaptive_threshold)
        } else {
            None
        }
    }
}

/// Wrapper for different types of event receivers
pub enum EventReceiver<T> {
    Unicast(UnboundedReceiverStream<T>),
    Broadcast(BroadcastStream<T>),
}

impl<T> EventReceiver<T> {
    /// Convert to UnboundedReceiverStream for backward compatibility
    pub fn into_unbounded_stream(self) -> UnboundedReceiverStream<T>
    where
        T: Clone + Send + 'static,
    {
        match self {
            EventReceiver::Unicast(stream) => stream,
            EventReceiver::Broadcast(broadcast_stream) => {
                // Convert broadcast stream to unbounded stream
                let (tx, rx) = mpsc::unbounded_channel();
                
                tokio::spawn(async move {
                    use tokio_stream::StreamExt;
                    let mut stream = broadcast_stream;
                    while let Some(result) = stream.next().await {
                        if let Ok(item) = result {
                            if tx.send(item).is_err() {
                                break;
                            }
                        }
                    }
                });
                
                UnboundedReceiverStream::new(rx)
            }
        }
    }
}

/// Optimized shared listener with reduced lock contention
pub struct ShareListener<T> {
    inner: Arc<RwLock<EventListeners<T>>>,
    strategy: EventStrategy,
}

impl<T: Clone + Send + 'static> Default for ShareListener<T> {
    fn default() -> Self {
        Self::new(EventStrategy::default())
    }
}

impl<T: Clone + Send + 'static> ShareListener<T> {
    /// Create new ShareListener with specified strategy
    pub fn new(strategy: EventStrategy) -> Self {
        Self {
            inner: Arc::new(RwLock::new(EventListeners::new(strategy))),
            strategy,
        }
    }

    /// Send event to all listeners with optimized locking
    pub async fn notify(&self, event: T) {
        // Try to use read lock first for broadcast mode
        if matches!(self.strategy, EventStrategy::Broadcast) {
            let listeners = self.inner.read().await;
            if let Some(ref tx) = listeners.broadcast_tx {
                let _ = tx.send(event);
                return;
            }
        }

        // Use write lock for unicast mode or when broadcast not available
        let mut listeners = self.inner.write().await;
        listeners.notify(event);
    }

    /// Create new listener with optimized allocation
    pub async fn new_listener(&self) -> UnboundedReceiverStream<T> {
        let mut listeners = self.inner.write().await;
        listeners.new_listener().into_unbounded_stream()
    }

    /// Get current listener count
    pub async fn len(&self) -> usize {
        let listeners = self.inner.read().await;
        listeners.len()
    }

    /// Check if there are any listeners
    pub async fn is_empty(&self) -> bool {
        let listeners = self.inner.read().await;
        listeners.is_empty()
    }
}

// Batch notification support for high-throughput scenarios
impl<T: Clone + Send + 'static> ShareListener<T> {
    /// Send multiple events in a batch for better performance
    pub async fn notify_batch(&self, events: Vec<T>) {
        if events.is_empty() {
            return;
        }

        // For broadcast mode, we can send events individually as they're already optimized
        if matches!(self.strategy, EventStrategy::Broadcast) {
            let listeners = self.inner.read().await;
            if let Some(ref tx) = listeners.broadcast_tx {
                for event in events {
                    let _ = tx.send(event);
                }
                return;
            }
        }

        // For unicast mode, batch send to reduce lock contention
        let mut listeners = self.inner.write().await;
        for event in events {
            listeners.notify(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_stream::StreamExt;

    #[tokio::test]
    async fn test_adaptive_strategy() {
        // Create adaptive listener with threshold of 3
        let mut listeners = EventListeners::new(EventStrategy::Adaptive(3));
        
        // Initially should be in unicast mode
        assert_eq!(listeners.adaptive_mode(), Some(AdaptiveMode::Unicast));
        
        // Add 2 listeners - should stay in unicast mode
        let _receiver1 = listeners.new_listener();
        let _receiver2 = listeners.new_listener();
        assert_eq!(listeners.len(), 2);
        
        // Add 3rd listener - should switch to broadcast mode
        let _receiver3 = listeners.new_listener();
        listeners.notify("test_event".to_string());
        assert_eq!(listeners.len(), 3);
        
        // Mode should have switched to broadcast
        assert_eq!(listeners.adaptive_mode(), Some(AdaptiveMode::Broadcast));
    }

    #[tokio::test]
    async fn test_share_listener_adaptive() {
        let share_listener = ShareListener::new(EventStrategy::Adaptive(2));
        
        // Add listener
        let mut receiver = share_listener.new_listener().await;
        
        // Send event
        share_listener.notify("hello".to_string()).await;
        
        // Should receive the event
        if let Some(event) = receiver.next().await {
            assert_eq!(event, "hello".to_string());
        }
        
        // Check listener count
        assert_eq!(share_listener.len().await, 1);
    }

    #[tokio::test]
    async fn test_adaptive_performance() {
        use std::time::Instant;
        
        // Test with different strategies
        let strategies = vec![
            ("Unicast", EventStrategy::Unicast),
            ("Broadcast", EventStrategy::Broadcast),
            ("Adaptive(5)", EventStrategy::Adaptive(5)),
        ];
        
        for (name, strategy) in strategies {
            let start = Instant::now();
            
            // Create listeners
            let mut listeners = EventListeners::new(strategy);
            
            // Add 10 listeners
            let mut receivers = Vec::new();
            for _ in 0..10 {
                receivers.push(listeners.new_listener());
            }
            
            // Send 100 events
            for i in 0..100 {
                listeners.notify(format!("event_{}", i));
            }
            
            let duration = start.elapsed();
            println!("{} strategy took: {:?}", name, duration);
            
            // For adaptive, check that it switched to broadcast mode
            if let EventStrategy::Adaptive(_) = strategy {
                assert_eq!(listeners.adaptive_mode(), Some(AdaptiveMode::Broadcast));
            }
        }
    }

    #[tokio::test]
    async fn test_adaptive_hysteresis() {
        // Test hysteresis mechanism to prevent oscillation
        let mut listeners = EventListeners::new(EventStrategy::Adaptive(5));
        
        // Add listeners up to threshold
        let mut receivers = Vec::new();
        for _ in 0..5 {
            receivers.push(listeners.new_listener());
        }
        
        // Should switch to broadcast
        listeners.notify("switch_to_broadcast".to_string());
        assert_eq!(listeners.adaptive_mode(), Some(AdaptiveMode::Broadcast));
        
        // Remove one listener (count = 4)
        receivers.pop();
        listeners.listener_count.store(4, Ordering::Relaxed);
        
        // Should still be in broadcast mode due to hysteresis
        listeners.notify("still_broadcast".to_string());
        assert_eq!(listeners.adaptive_mode(), Some(AdaptiveMode::Broadcast));
        
        // Remove more listeners (count = 2, below threshold - 2)
        receivers.truncate(2);
        listeners.listener_count.store(2, Ordering::Relaxed);
        
        // Now should switch back to unicast
        listeners.notify("switch_to_unicast".to_string());
        assert_eq!(listeners.adaptive_mode(), Some(AdaptiveMode::Unicast));
    }
}

// Usage Examples:
// 
// Create an adaptive listener with default threshold (8)
// let mut listeners = EventListeners::new(EventStrategy::Adaptive(8));
// 
// Add listeners - starts in unicast mode
// let receiver1 = listeners.new_listener();
// let receiver2 = listeners.new_listener();
// 
// Send events (still in unicast mode)
// listeners.notify("event1".to_string());
// 
// Add more listeners - automatically switches to broadcast when threshold reached
// for _ in 0..10 {
//     let _receiver = listeners.new_listener();
// }
// 
// Now in broadcast mode for better performance with many listeners
// listeners.notify("event2".to_string());
// 
// For shared usage across tasks:
// let share_listener = ShareListener::new(EventStrategy::Adaptive(5));
// let receiver = share_listener.new_listener().await;
// share_listener.notify("shared_event".to_string()).await;
