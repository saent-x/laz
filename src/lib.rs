//! # Laz - Type-Safe RPC Library for Rust
//! 
//! Laz is a comprehensive RPC library that provides type-safe remote procedure calls
//! with automatic code generation, Loco.rs integration, and compile-time endpoint discovery.
//! 
//! ## Features
//! 
//! - **server**: Server-side RPC functionality with Loco.rs integration
//! - **client**: Client-side RPC functionality with automatic code generation  
//! - **schema**: Schema derivation macros
//! - **full**: All features enabled
//! 
//! ## Usage
//! 
//! ### Server with Loco.rs
//! ```toml
//! [dependencies]
//! laz = { version = "0.1.0", features = ["server"] }
//! ```
//! 
//! ### Client Application
//! ```toml
//! [dependencies]
//! laz = { version = "0.1.0", features = ["client"] }
//! ```
//! 
//! ### Full Stack
//! ```toml
//! [dependencies]
//! laz = { version = "0.1.0", features = ["full"] }
//! ```

// Core types (always available)
pub use laz_types::*;

// Client generation utilities
#[cfg(feature = "client")]
mod client_gen;

// Server functionality
#[cfg(feature = "server")]
pub mod server {
    pub use laz_server::*;
    pub use laz_server_macros::{rpc_query, rpc_mutation};
}

// Client functionality
#[cfg(feature = "client")]
pub mod client {
    pub use laz_client::*;
    
    // Re-export the basic client functionality
    pub use laz_client::{LocoClient, ServerAddr, RpcClientError, RpcFunction};
    
    // Generate the RPC client and all associated types
    // This macro will generate GeneratedRpcClient and all the types like LoginParams, etc.
    laz_client_macros::generate_rpc_client!();
}


// Schema functionality
#[cfg(feature = "schema")]
pub use laz_schema_derive::LazSchema;

// Re-export commonly used items at the root
pub use serde;
pub use serde_json;
pub use thiserror;

#[cfg(feature = "server")]
pub use axum;

#[cfg(feature = "server")]
pub use loco_rs;

#[cfg(feature = "client")]
pub use laz_client::reqwest;

// Convenience re-exports for server
#[cfg(feature = "server")]
pub mod prelude {
    pub use crate::server::*;
    pub use crate::server::{rpc_query, rpc_mutation};
    pub use crate::LazSchema;
}

// Convenience re-exports for client
#[cfg(feature = "client")]
pub mod client_prelude {
    pub use crate::client::*;
}

// Convenience re-exports for full stack
#[cfg(feature = "full")]
pub mod full_prelude {
    pub use crate::prelude::*;
    pub use crate::client_prelude::*;
    pub use crate::LazSchema;
}
