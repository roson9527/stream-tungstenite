# stream-tungstenite

## Overview

`stream-tungstenite` is a powerful Rust WebSocket client library designed for applications requiring reliable connections. It provides automatic reconnection, connection state management, extension mechanisms, and detailed metrics collection.

## Core Features

- **Intelligent Reconnection**: Multiple reconnection strategies including exponential backoff algorithm
- **Connection State Management**: Real-time tracking of connection status and health
- **Extension Mechanism**: Support for registering custom extensions to handle specific business logic
- **Event Streams**: Provides message receiving streams and status change streams
- **Metrics Collection**: Detailed connection metrics and error statistics
- **Configuration Presets**: Pre-configured options for different scenarios

## Quick Start

### Basic Usage

```rust
use stream_tungstenite::prelude::*;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    // Create client with default configuration
    let client = Arc::new(ReconnectT::new("wss://echo.websocket.org", None));
    
    // Create message receiving stream
    let mut receive_stream = client.create_receive_stream().await;
    
    // Run client in background
    let client_clone = client.clone();
    tokio::spawn(async move {
        client_clone.run().await;
    });
    
    // Send message
    let message = tokio_tungstenite::tungstenite::Message::Text("Hello WebSocket!".into());
    client.sender.send(message).await.unwrap();
    
    // Receive message
    if let Some(received) = receive_stream.next().await {
        println!("Received message: {:?}", received);
    }
}
```

### Using Configuration Presets

```rust
use stream_tungstenite::prelude::*;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    // Use fast reconnection configuration (from src/config.rs)
    let config = ReconnectOptions::fast_reconnect();
    let client = Arc::new(ReconnectT::new("wss://echo.websocket.org", Some(config)));
    
    client.run().await;
}
```

### Monitoring Connection Status

```rust
use stream_tungstenite::prelude::*;
use futures_util::StreamExt;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let client = Arc::new(ReconnectT::new("wss://echo.websocket.org", None));
    
    // Create status stream
    let mut status_stream = client.create_status_stream().await;
    
    // Start client
    let client_clone = client.clone();
    tokio::spawn(async move {
        client_clone.run().await;
    });
    
    // Monitor status changes
    while let Some(status) = status_stream.next().await {
        match status {
            ConnectionStatus::Connected => println!("✅ Connected"),
            ConnectionStatus::Disconnected => println!("❌ Disconnected"),
        }
    }
}
```

## Advanced Configuration

### Custom Reconnection Strategy

```rust
use stream_tungstenite::prelude::*;
use std::time::Duration;

let custom_strategy = ExpBackoffStrategy::new(
    Duration::from_millis(500),  // Initial delay
    2.0,                        // Growth factor
    0.1                         // Jitter factor
).with_max(Duration::from_secs(30));

let config = ReconnectOptions::default()
    .with_exp_backoff_strategy(custom_strategy)
    .with_receive_timeout(Duration::from_secs(15));

let client = ReconnectT::new("wss://example.com", Some(config));
```

### Configuration Preset Options

The library provides three preset configurations for different scenarios (from `src/config.rs`):

#### 1. Fast Reconnection - `fast_reconnect()`
Suitable for scenarios requiring quick connection recovery:
- Initial delay: 500ms
- Growth factor: 1.5
- Maximum delay: 10 seconds
- Receive timeout: 10 seconds

#### 2. Stable Connection - `stable_connection()`
Suitable for scenarios requiring long-term stable connections:
- Initial delay: 2 seconds
- Growth factor: 2.0
- Maximum delay: 120 seconds
- Receive timeout: 60 seconds

#### 3. Low Latency - `low_latency()`
Suitable for latency-sensitive scenarios:
- Initial delay: 100ms
- Growth factor: 1.2
- Maximum delay: 5 seconds
- Receive timeout: 5 seconds

### Extension Mechanism

You can add custom functionality by implementing the `Extension` trait:

```rust
use stream_tungstenite::prelude::*;

// Register extension
let client = Arc::new(ReconnectT::new("wss://example.com", None));
client.register_extension(your_extension).await.unwrap();
```

## Connection State and Metrics

### Getting Connection State

```rust
// Get connection state snapshot (from src/tungstenite.rs)
let state = client.get_connection_state().await;
println!("Connection ID: {}", state.connection_id);
println!("Status: {:?}", state.status);
println!("Reconnect count: {}", state.reconnect_count);
println!("Error count: {}", state.error_count);
```

### Health Check

```rust
// Check if connection is healthy
let is_healthy = client.is_connection_healthy();
println!("Connection healthy: {}", is_healthy);
```

## Error Handling

The library provides structured error handling mechanisms (from `src/errors.rs`):

```rust
use stream_tungstenite::prelude::*;

// Errors are automatically recorded in connection state
let state = client.get_connection_state().await;
if let Some(last_error) = state.last_error {
    println!("Last error: {:?}", last_error);
}
```

## Reconnection Strategy Details

### Exponential Backoff Strategy

`ExpBackoffStrategy` provides flexible exponential backoff reconnection mechanism (from `src/strategies.rs`):

```rust
use stream_tungstenite::prelude::*;
use std::time::Duration;

let strategy = ExpBackoffStrategy::new(
    Duration::from_secs(1),    // Initial delay
    2.0,                       // Double delay each retry
    0.05                       // 5% random jitter
)
.with_max(Duration::from_secs(60))  // Maximum delay not exceeding 60 seconds
.with_seed(12345);                  // Optional: set random seed

let config = ReconnectOptions::default()
    .with_exp_backoff_strategy(strategy);
```

### Strategy Features

- **Exponential Growth**: Delay time grows exponentially to avoid frequent retries
- **Jitter Mechanism**: Add random jitter to avoid multiple clients retrying simultaneously
- **Maximum Delay**: Set delay ceiling to prevent excessive wait times
- **Resettable**: Support resetting strategy state to start over

## Installation

Add dependency to your `Cargo.toml`:

```toml
[dependencies]
stream-tungstenite = "0.4.0"
tokio = { version = "1.0", features = ["full"] }
```

## License

This project is licensed under the Apache License Version 2.0. See the LICENSE file for details.

## Contributing

Contributions are welcome! Please feel free to submit Pull Requests or create Issues to report bugs and suggest improvements.