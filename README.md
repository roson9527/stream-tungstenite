# stream-tungstenite

## Overview

`stream-tungstenite` is a Rust library designed to facilitate the management of WebSocket connections with automatic reconnection capabilities. It provides a robust framework for handling WebSocket communication, including error handling, event listening, and customizable reconnection strategies.

## Problem Statement

WebSocket connections can be unstable due to network issues, server downtime, or other unforeseen circumstances. This library addresses the following challenges:

- **Automatic Reconnection**: Automatically attempts to reconnect when the connection is lost.
- **Error Handling**: Provides structured error handling for various connection issues.
- **Event Management**: Allows for easy management of events through listeners.

## Features

- **Reconnect Options**: Customize the reconnection strategy with options such as retry intervals and timeouts.
- **Event Listeners**: Register multiple listeners to handle incoming messages and events.
- **Handshake Support**: Implement custom handshake logic if needed.

## Example Usage

Here’s a simple example of how to use `stream-tungstenite` to establish a WebSocket connection and handle messages:

```rust
use stream_tungstenite::{ReconnectT, ReconnectOptions};
use tokio_tungstenite::tungstenite::Message;
#[tokio::main]
async fn main() {
    let options = ReconnectOptions::default();
    let reconnect = ReconnectT::new("wss://example.com/socket", Some(options));
    // Start the connection
    reconnect.run().await;
}
```

In this example, a new `ReconnectT` instance is created with a specified WebSocket URL and default options. The `run` method initiates the connection and handles reconnections automatically.

## Installation

To include `stream-tungstenite` in your project, add the following to your `Cargo.toml`:

```toml
[dependencies]
stream-tungstenite = "0.4.0"
```

## Contributing

Contributions are welcome! Please feel free to submit a pull request or open an issue for any enhancements or bug fixes.

## License

This project is licensed under the Apache License Version 2.0. See the LICENSE file for more details.