# Laz - Type-Safe RPC Library for Rust

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Status](https://img.shields.io/badge/status-WIP-yellow)](https://github.com/saent-x/laz)

> **⚠️ Work in Progress**: Laz is currently under active development and is not yet ready for production use. APIs may change, and some features may be incomplete.

Laz is a comprehensive, type-safe RPC library for Rust that provides automatic code generation, Loco.rs integration, and compile-time endpoint discovery. It enables seamless communication between Rust applications with full type safety and minimal boilerplate.

## Features

- **🔒 Type Safety**: Compile-time type checking for all RPC calls
- **⚡ Automatic Code Generation**: Generate client code from server metadata
- **🚀 Loco.rs Integration**: Built-in support for Loco.rs web framework
- **📡 Compile-time Discovery**: Automatic endpoint discovery at compile time
- **🎯 Minimal Boilerplate**: Clean, intuitive API
- **🔧 Flexible Architecture**: Use only what you need

## Installation

Laz is not published to crates.io yet (the `laz` name there belongs to an unrelated LiDAR crate), so depend on it from Git and pick the features you need:

### For Server Applications (Loco.rs)

```toml
[dependencies]
laz = { git = "https://github.com/saent-x/laz", features = ["server"] }
```

### For Client Applications

```toml
[dependencies]
laz = { git = "https://github.com/saent-x/laz", features = ["client"] }
```

### For Schema Derivation

```toml
[dependencies]
laz = { git = "https://github.com/saent-x/laz", features = ["schema"] }
```

### For Full Stack Development

```toml
[dependencies]
laz = { git = "https://github.com/saent-x/laz", features = ["full"] }
```

### Core Types Only

```toml
[dependencies]
laz = { git = "https://github.com/saent-x/laz" }
```

## Quick Start

### Server Side (Loco.rs)

Define your RPC functions using the `rpc_query` and `rpc_mutation` macros:

```rust
use laz::prelude::*;

#[rpc_query]
pub async fn hello() -> Result<String, RpcError> {
    Ok("Hello from Laz RPC!".to_string())
}

#[rpc_mutation]
pub async fn register(params: RegisterParams) -> Result<User, RpcError> {
    // Your registration logic here
    Ok(User {
        id: 1,
        email: params.email,
        name: params.name,
    })
}

// Add the LazSchema derive to your types
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, LazSchema)]
pub struct RegisterParams {
    pub email: String,
    pub password: String,
    pub name: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, LazSchema)]
pub struct User {
    pub id: u64,
    pub email: String,
    pub name: String,
}
```

### Client Side

Generate a dynamic RPC client that discovers functions at runtime:

```rust
use laz::client_prelude::*;

// Generate the RPC client
generate_rpc_client!();

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the client
    let server_addr = laz::client::ServerAddr {
        ip: "localhost".to_string(),
        port: 3000,
    };
    let client = RpcClient::init(server_addr).await?;

    // Discover available functions
    let functions = client.available_functions();
    println!("Available functions: {:?}", functions);

    // Call RPC functions dynamically
    let hello_response = client.call("hello", None).await?;
    println!("Hello response: {:?}", hello_response);

    // Call with parameters
    let params = serde_json::json!({
        "email": "user@example.com",
        "password": "secure_password",
        "name": "John Doe"
    });
    let result = client.call("register", Some(params)).await?;
    println!("Registration result: {:?}", result);

    Ok(())
}
```

For fully type-safe function calls, the client will automatically generate typed wrapper functions based on server metadata when connected.

## Architecture

Laz is organized into several focused crates that work together:

### Core Components

- **`laz_types`**: Shared types and schemas used by both server and client
- **`laz_server`**: Server-side RPC framework with Loco.rs integration
- **`laz_client`**: Client-side RPC framework with automatic code generation
- **`laz_server_macros`**: Procedural macros for server-side RPC functions
- **`laz_client_macros`**: Procedural macros for client-side code generation
- **`laz_schema_derive`**: Derive macro for automatic schema generation

### Feature Flags

- **`server`**: Enables server-side functionality with Loco.rs integration
- **`client`**: Enables client-side functionality with code generation
- **`schema`**: Enables schema derivation macros
- **`full`**: Enables all features

## Advanced Usage

### Custom Error Handling

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MyError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("RPC error: {0}")]
    Rpc(#[from] RpcError),
}

#[rpc_query]
pub async fn get_user(id: u64) -> Result<User, MyError> {
    // Your logic here
    Ok(User { /* ... */ })
}
```

### Type-Safe Parameters

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, LazSchema)]
pub struct PaginationParams {
    pub page: u32,
    pub per_page: u32,
}

#[rpc_query]
pub async fn list_users(params: PaginationParams) -> Result<Vec<User>, RpcError> {
    // Your pagination logic here
    Ok(vec![])
}
```

### Async Mutations

```rust
#[rpc_mutation]
pub async fn update_user(id: u64, updates: UserUpdates) -> Result<User, RpcError> {
    // Your update logic here
    Ok(updated_user)
}
```

## Examples

Runnable examples (Loco.rs server, typed client, shared types) are being added. Until then, the snippets in this README are the reference.

## Documentation

- **API documentation**: run `cargo doc --open` in this repository

## Performance

Laz is designed for high performance:

- **Zero-copy serialization**: Uses serde for efficient serialization
- **Compile-time optimization**: All type checking happens at compile time
- **Minimal runtime overhead**: Generated code is optimized for performance
- **Connection pooling**: Built-in connection pooling for client applications

## Compatibility

- **Rust Version**: 1.70.0 or later
- **Tokio**: Full async/await support
- **Serde**: Compatible with all serde formats
- **Loco.rs**: Integrated with Loco.rs web framework

## Contributing

We welcome contributions! Please see our [Contributing Guide](CONTRIBUTING.md) for details.

### Development Setup

```bash
git clone https://github.com/saent-x/laz.git
cd laz
cargo build --all-features
cargo test --all-features
```

## License

This project is licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Acknowledgments

- Built with [Loco.rs](https://loco.rs/) for the server framework
- Uses [Tokio](https://tokio.rs/) for async runtime
- Powered by [Serde](https://serde.rs/) for serialization
- Inspired by [gRPC](https://grpc.io/) and [GraphQL](https://graphql.org/)

## Roadmap

- [ ] WebSocket support for real-time RPC
- [ ] Streaming RPC calls
- [ ] Authentication middleware
- [ ] Rate limiting
- [ ] Metrics and monitoring
- [ ] OpenAPI specification generation
- [ ] TypeScript client generation
- [ ] Python client generation

---

**Laz** - Making RPC in Rust as easy as it should be. 🚀
