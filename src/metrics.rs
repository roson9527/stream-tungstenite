use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use std::collections::VecDeque;
use tokio::sync::RwLock;

/// Performance metrics configuration
#[derive(Debug, Clone)]
pub struct MetricsConfig {
    /// Whether to collect metrics
    pub enabled: bool,
    /// Maximum number of latency samples to keep
    pub max_samples: usize,
    /// Interval for calculating statistics
    pub stats_interval: Duration,
    /// Whether to include detailed timing information
    pub detailed_timing: bool,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Disabled by default for performance
            max_samples: 1000,
            stats_interval: Duration::from_secs(60),
            detailed_timing: false,
        }
    }
}

impl MetricsConfig {
    /// Enable metrics collection with default settings
    pub fn enabled() -> Self {
        Self {
            enabled: true,
            ..Default::default()
        }
    }

    /// Enable metrics with detailed timing
    pub fn detailed() -> Self {
        Self {
            enabled: true,
            detailed_timing: true,
            ..Default::default()
        }
    }

    /// Enable metrics with custom sample size
    pub fn with_sample_size(mut self, max_samples: usize) -> Self {
        self.max_samples = max_samples;
        self
    }

    /// Enable metrics with custom stats interval
    pub fn with_stats_interval(mut self, interval: Duration) -> Self {
        self.stats_interval = interval;
        self
    }
}

/// Simplified connection metrics
#[derive(Debug, Clone)]
pub struct ConnectionMetrics {
    /// Connection ID
    pub connection_id: u64,
    /// Total number of connections attempted
    pub connection_attempts: u64,
    /// Total number of successful connections
    pub successful_connections: u64,
    /// Total number of failed connections
    pub failed_connections: u64,
    /// Current connection duration
    pub current_connection_duration: Option<Duration>,
    /// Total time connected
    pub total_connected_time: Duration,
    /// Number of reconnections
    pub reconnect_count: u64,
    /// Connection success rate (0.0 to 1.0)
    pub success_rate: f64,
    /// Average connection duration
    pub avg_connection_duration: Duration,
}

impl Default for ConnectionMetrics {
    fn default() -> Self {
        Self {
            connection_id: 0,
            connection_attempts: 0,
            successful_connections: 0,
            failed_connections: 0,
            current_connection_duration: None,
            total_connected_time: Duration::ZERO,
            reconnect_count: 0,
            success_rate: 0.0,
            avg_connection_duration: Duration::ZERO,
        }
    }
}

/// Simplified throughput metrics
#[derive(Debug, Clone)]
pub struct ThroughputMetrics {
    /// Total messages sent
    pub messages_sent: u64,
    /// Total messages received
    pub messages_received: u64,
    /// Total bytes sent
    pub bytes_sent: u64,
    /// Total bytes received
    pub bytes_received: u64,
    /// Messages per second (recent average)
    pub messages_per_second: f64,
    /// Bytes per second (recent average)
    pub bytes_per_second: f64,
    /// Peak messages per second
    pub peak_messages_per_second: f64,
    /// Peak bytes per second
    pub peak_bytes_per_second: f64,
}

impl Default for ThroughputMetrics {
    fn default() -> Self {
        Self {
            messages_sent: 0,
            messages_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
            messages_per_second: 0.0,
            bytes_per_second: 0.0,
            peak_messages_per_second: 0.0,
            peak_bytes_per_second: 0.0,
        }
    }
}

/// Simplified latency statistics
#[derive(Debug, Clone)]
pub struct LatencyStats {
    /// Average latency
    pub avg: Duration,
    /// Minimum latency
    pub min: Duration,
    /// Maximum latency
    pub max: Duration,
    /// 50th percentile (median)
    pub p50: Duration,
    /// 95th percentile
    pub p95: Duration,
    /// 99th percentile
    pub p99: Duration,
    /// Number of samples
    pub sample_count: usize,
}

impl Default for LatencyStats {
    fn default() -> Self {
        Self {
            avg: Duration::ZERO,
            min: Duration::ZERO,
            max: Duration::ZERO,
            p50: Duration::ZERO,
            p95: Duration::ZERO,
            p99: Duration::ZERO,
            sample_count: 0,
        }
    }
}

/// Simplified performance metrics
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    /// Connection-related metrics
    pub connection: ConnectionMetrics,
    /// Message throughput metrics
    pub throughput: ThroughputMetrics,
    /// Latency statistics
    pub latency: LatencyStats,
    /// When these metrics were collected
    pub timestamp: std::time::SystemTime,
    /// Uptime since metrics collection started
    pub uptime: Duration,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            connection: ConnectionMetrics::default(),
            throughput: ThroughputMetrics::default(),
            latency: LatencyStats::default(),
            timestamp: std::time::SystemTime::now(),
            uptime: Duration::ZERO,
        }
    }
}

/// Simplified metrics collector with standard atomic types
pub struct MetricsCollector {
    config: MetricsConfig,
    
    // Connection metrics
    connection_attempts: AtomicU64,
    successful_connections: AtomicU64,
    failed_connections: AtomicU64,
    reconnect_count: AtomicU64,
    
    // Throughput metrics
    messages_sent: AtomicU64,
    messages_received: AtomicU64,
    bytes_sent: AtomicU64,
    bytes_received: AtomicU64,
    
    // Timing data
    start_time: Instant,
    current_connection_start: Arc<RwLock<Option<Instant>>>,
    total_connected_time: Arc<RwLock<Duration>>,
    
    // Simplified latency tracking
    latency_samples: Arc<RwLock<VecDeque<Duration>>>,
    
    // Throughput calculation with mutex-protected state
    last_stats: Arc<RwLock<(Instant, u64, u64, f64, f64)>>, // (time, messages, bytes, msg_rate, byte_rate)
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new(config: MetricsConfig) -> Self {
        let now = Instant::now();
        Self {
            config,
            connection_attempts: AtomicU64::new(0),
            successful_connections: AtomicU64::new(0),
            failed_connections: AtomicU64::new(0),
            reconnect_count: AtomicU64::new(0),
            messages_sent: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),
            bytes_sent: AtomicU64::new(0),
            bytes_received: AtomicU64::new(0),
            start_time: now,
            current_connection_start: Arc::new(RwLock::new(None)),
            total_connected_time: Arc::new(RwLock::new(Duration::ZERO)),
            latency_samples: Arc::new(RwLock::new(VecDeque::new())),
            last_stats: Arc::new(RwLock::new((now, 0, 0, 0.0, 0.0))),
        }
    }

    /// Check if metrics collection is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Record connection attempt
    pub fn record_connection_attempt(&self) {
        if !self.config.enabled {
            return;
        }
        self.connection_attempts.fetch_add(1, Ordering::Relaxed);
    }

    /// Record successful connection
    pub async fn record_connection_success(&self) {
        if !self.config.enabled {
            return;
        }
        
        self.successful_connections.fetch_add(1, Ordering::Relaxed);
        let mut start = self.current_connection_start.write().await;
        *start = Some(Instant::now());
    }

    /// Record failed connection
    pub fn record_connection_failure(&self) {
        if !self.config.enabled {
            return;
        }
        self.failed_connections.fetch_add(1, Ordering::Relaxed);
    }

    /// Record disconnection
    pub async fn record_disconnection(&self) {
        if !self.config.enabled {
            return;
        }
        
        let mut start = self.current_connection_start.write().await;
        if let Some(connection_start) = start.take() {
            let duration = connection_start.elapsed();
            let mut total = self.total_connected_time.write().await;
            *total += duration;
        }
    }

    /// Record reconnection
    pub fn record_reconnection(&self) {
        if !self.config.enabled {
            return;
        }
        self.reconnect_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Record message sent
    pub async fn record_message_sent(&self, bytes: usize) {
        if !self.config.enabled {
            return;
        }
        
        self.messages_sent.fetch_add(1, Ordering::Relaxed);
        self.bytes_sent.fetch_add(bytes as u64, Ordering::Relaxed);
        
        self.update_throughput_stats().await;
    }

    /// Record message received
    pub async fn record_message_received(&self, bytes: usize) {
        if !self.config.enabled {
            return;
        }
        
        self.messages_received.fetch_add(1, Ordering::Relaxed);
        self.bytes_received.fetch_add(bytes as u64, Ordering::Relaxed);
        
        self.update_throughput_stats().await;
    }

    /// Record latency sample
    pub async fn record_latency(&self, latency: Duration) {
        if !self.config.enabled {
            return;
        }
        
        let mut samples = self.latency_samples.write().await;
        samples.push_back(latency);
        
        if samples.len() > self.config.max_samples {
            samples.pop_front();
        }
    }

    /// Update throughput statistics
    async fn update_throughput_stats(&self) {
        let now = Instant::now();
        let current_messages = self.messages_sent.load(Ordering::Relaxed) + self.messages_received.load(Ordering::Relaxed);
        let current_bytes = self.bytes_sent.load(Ordering::Relaxed) + self.bytes_received.load(Ordering::Relaxed);
        
        let mut stats = self.last_stats.write().await;
        let (last_time, last_messages, last_bytes, ref mut peak_msg_rate, ref mut peak_byte_rate) = &mut *stats;
        
        let time_diff = now.duration_since(*last_time).as_secs_f64();
        if time_diff >= 1.0 {
            let msg_rate = (current_messages - *last_messages) as f64 / time_diff;
            let byte_rate = (current_bytes - *last_bytes) as f64 / time_diff;
            
            *peak_msg_rate = peak_msg_rate.max(msg_rate);
            *peak_byte_rate = peak_byte_rate.max(byte_rate);
            
            *last_time = now;
            *last_messages = current_messages;
            *last_bytes = current_bytes;
        }
    }

    /// Calculate latency statistics
    async fn calculate_latency_stats(&self) -> LatencyStats {
        let samples = self.latency_samples.read().await;
        
        if samples.is_empty() {
            return LatencyStats::default();
        }
        
        let mut sorted: Vec<Duration> = samples.iter().cloned().collect();
        sorted.sort();
        
        let count = sorted.len();
        let sum: Duration = sorted.iter().sum();
        let avg = sum / count as u32;
        
        let min = sorted[0];
        let max = sorted[count - 1];
        let p50 = sorted[count / 2];
        let p95 = sorted[(count * 95) / 100];
        let p99 = sorted[(count * 99) / 100];
        
        LatencyStats {
            avg,
            min,
            max,
            p50,
            p95,
            p99,
            sample_count: count,
        }
    }

    /// Get current metrics
    pub async fn get_metrics(&self, connection_id: u64) -> PerformanceMetrics {
        let attempts = self.connection_attempts.load(Ordering::Relaxed);
        let successful = self.successful_connections.load(Ordering::Relaxed);
        let failed = self.failed_connections.load(Ordering::Relaxed);
        let reconnects = self.reconnect_count.load(Ordering::Relaxed);
        
        let success_rate = if attempts > 0 {
            successful as f64 / attempts as f64
        } else {
            0.0
        };
        
        let current_connection_duration = {
            let start = self.current_connection_start.read().await;
            start.map(|s| s.elapsed())
        };
        
        let total_connected_time = *self.total_connected_time.read().await;
        let avg_connection_duration = if successful > 0 {
            total_connected_time / successful as u32
        } else {
            Duration::ZERO
        };
        
        let connection = ConnectionMetrics {
            connection_id,
            connection_attempts: attempts,
            successful_connections: successful,
            failed_connections: failed,
            current_connection_duration,
            total_connected_time,
            reconnect_count: reconnects,
            success_rate,
            avg_connection_duration,
        };
        
        let stats = self.last_stats.read().await;
        let (_, _, _, peak_msg_rate, peak_byte_rate) = *stats;
        
        let throughput = ThroughputMetrics {
            messages_sent: self.messages_sent.load(Ordering::Relaxed),
            messages_received: self.messages_received.load(Ordering::Relaxed),
            bytes_sent: self.bytes_sent.load(Ordering::Relaxed),
            bytes_received: self.bytes_received.load(Ordering::Relaxed),
            messages_per_second: 0.0, // Simplified - would need more complex calculation
            bytes_per_second: 0.0, // Simplified - would need more complex calculation
            peak_messages_per_second: peak_msg_rate,
            peak_bytes_per_second: peak_byte_rate,
        };
        
        let latency = self.calculate_latency_stats().await;
        
        PerformanceMetrics {
            connection,
            throughput,
            latency,
            timestamp: std::time::SystemTime::now(),
            uptime: self.start_time.elapsed(),
        }
    }

    /// Reset all metrics
    pub async fn reset(&self) {
        self.connection_attempts.store(0, Ordering::Relaxed);
        self.successful_connections.store(0, Ordering::Relaxed);
        self.failed_connections.store(0, Ordering::Relaxed);
        self.reconnect_count.store(0, Ordering::Relaxed);
        self.messages_sent.store(0, Ordering::Relaxed);
        self.messages_received.store(0, Ordering::Relaxed);
        self.bytes_sent.store(0, Ordering::Relaxed);
        self.bytes_received.store(0, Ordering::Relaxed);
        
        let mut start = self.current_connection_start.write().await;
        *start = None;
        
        let mut total = self.total_connected_time.write().await;
        *total = Duration::ZERO;
        
        let mut samples = self.latency_samples.write().await;
        samples.clear();
        
        let mut stats = self.last_stats.write().await;
        *stats = (Instant::now(), 0, 0, 0.0, 0.0);
    }
} 