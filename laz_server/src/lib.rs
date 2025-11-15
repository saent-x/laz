//! Laz Server - Type-safe RPC framework for Loco.rs
//! 
//! This crate provides server-side RPC functionality with automatic type generation
//! and seamless integration with Loco.rs applications.

use async_trait::async_trait;
use axum::{routing::get, Json};
use loco_rs::{
    app::{AppContext, Initializer},
    Result,
};
use serde_json::Value;
use std::sync::OnceLock;

pub use laz_types::*;

/// Re-export the server macros and LazSchema derive
pub use laz_server_macros::{rpc_query, rpc_mutation};
pub use laz_schema_derive::LazSchema;

/// Global registry for endpoint discovery
static ENDPOINTS_DISCOVERY: OnceLock<Vec<(String, Vec<String>)>> = OnceLock::new();

/// Initializer that exposes RPC metadata via HTTP endpoint
pub struct LazEndpoint;

#[async_trait]
impl Initializer for LazEndpoint {
    fn name(&self) -> String {
        "laz-endpoint".to_string()
    }

    /// Mounts the RPC metadata endpoint AFTER all routes are registered
    async fn after_routes(&self, router: axum::routing::Router, _ctx: &AppContext) -> Result<axum::routing::Router> {
        let meta_router = axum::Router::new().route(
            "/_laz/metadata",
            get(|| async move {
                let metadata = laz_types::get_all_function_metadata();
                let functions: Vec<Value> = metadata
                    .into_iter()
                    .map(|m| {
                        let input_schema_json = m
                            .input_type_name
                            .as_ref()
                            .and_then(|name| laz_types::find_type_schema(name))
                            .and_then(|schema| serde_json::to_string(schema).ok());
                        let output_schema_json = laz_types::find_type_schema(&m.output_type_name)
                            .and_then(|schema| serde_json::to_string(schema).ok());

                        serde_json::json!({
                            "function_name": m.function_name,
                            "is_mutation": m.is_mutation,
                            "is_async": m.is_async,
                            "input_type_name": m.input_type_name,
                            "output_type_name": m.output_type_name,
                            "params": m.params,
                            "input_schema_json": input_schema_json,
                            "output_schema_json": output_schema_json,
                        })
                    })
                    .collect();

                let endpoints_discovery = get_endpoints_discovery()
                    .map(|endpoints| {
                        endpoints.iter().map(|(uri, actions)| {
                            serde_json::json!({
                                "uri": uri,
                                "methods": actions
                            })
                        }).collect::<Vec<_>>()
                    })
                    .unwrap_or_default();

                Json(serde_json::json!({
                    "total_functions": functions.len(),
                    "functions": functions,
                    "endpoints_discovery": endpoints_discovery,
                    "total_endpoints": endpoints_discovery.len()
                }))
            }),
        );

        Ok(router.merge(meta_router))
    }
}

/// Get the endpoints discovery data for RPC metadata
pub fn get_endpoints_discovery() -> Option<&'static Vec<(String, Vec<String>)>> {
    ENDPOINTS_DISCOVERY.get()
}

/// Register endpoint discovery data
pub fn register_endpoints_discovery(endpoints: Vec<(String, Vec<String>)>) {
    let _ = ENDPOINTS_DISCOVERY.set(endpoints);
}

/// Helper to collect route information from Loco.rs AppRoutes
pub fn collect_routes(app_routes: &loco_rs::controller::AppRoutes) -> Vec<(String, Vec<String>)> {
    let mut endpoints = Vec::new();
    
    for route in app_routes.collect() {
        let mut actions = Vec::new();
        for action in &route.actions {
            actions.push(action.as_str().to_string());
        }
        endpoints.push((route.uri.to_string(), actions));
    }
    
    endpoints
}

/// Re-export commonly used items
pub mod prelude {
    pub use crate::{
        LazEndpoint, LazError, ServerAddr, FunctionMetadata, TypeSchema,
        get_all_function_metadata, get_all_type_schemas, find_type_schema,
        rpc_query, rpc_mutation, LazSchema,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_addr_creation() {
        let addr = ServerAddr {
            ip: "localhost".to_string(),
            port: 8080,
        };
        assert_eq!(addr.base_url(), "http://localhost:8080");
    }
}