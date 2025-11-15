//! Client generation utilities for Laz
//! 
//! This module provides utilities for generating type-safe RPC clients
//! that can be used by individual client applications.

/// Generate a type-safe RPC client for a specific server
/// 
/// This macro should be called in client applications to generate
/// a client specific to their target server.
#[macro_export]
macro_rules! generate_client_for_server {
    ($server_url:expr) => {
        // This will be expanded by the build script to include the actual generated code
        // For now, we'll use a placeholder that gets replaced during build
        include!(concat!(env!("OUT_DIR"), "/generated_rpc_client.rs"));
    };
}

/// Generate a type-safe RPC client using the default server URL
/// 
/// This macro uses the LAZ_SERVER_URL environment variable or defaults to localhost:5150
#[macro_export]
macro_rules! generate_default_client {
    () => {
        $crate::generate_client_for_server!(env!("LAZ_SERVER_URL", "http://localhost:5150"));
    };
}