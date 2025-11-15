//! Laz Client Library for Loco Framework
//! 
//! This library provides a client for calling RPC functions exposed by a Loco server.
//! The client automatically fetches metadata from the server's `/_laz/metadata` endpoint
//! and generates callable functions based on the available RPC endpoints.

pub mod client;

pub use client::{LocoClient, ServerAddr, RpcClientError, RpcFunction};
pub use laz_client_macros::{generate_rpc_client, create_rpc_client};
pub use reqwest;
