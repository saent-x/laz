//! Shared types for the laz RPC library.
//!
//! This crate provides the core types used by both laz_server and laz_client
//! for type-safe RPC communication.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Schema for any Rust type (struct, enum, primitive)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum TypeSchema {
    /// Primitive types: i32, String, bool, etc.
    Primitive(String),
    /// Structs with named fields
    Struct(StructSchema),
    /// Enums with variants
    Enum(EnumSchema),
    /// Generic container like Vec<T>, Option<T>
    Container {
        container_type: String,
        inner_type: Box<TypeSchema>,
    },
    /// Tuple types
    Tuple(Vec<Box<TypeSchema>>),
    /// Self-referencing or unresolvable types
    Opaque(String),
}

/// Schema for a struct
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructSchema {
    pub type_name: String,
    pub fields: Vec<FieldSchema>,
}

/// Single field in a struct
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSchema {
    pub field_name: String,
    pub field_type: Box<TypeSchema>,
    pub optional: bool,
}

/// Schema for an enum
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumSchema {
    pub type_name: String,
    pub variants: Vec<VariantSchema>,
}

/// Enum variant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantSchema {
    pub variant_name: String,
    pub inner_schema: Option<Box<TypeSchema>>,
}

/// Metadata for RPC functions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionMetadata {
    pub function_name: String,
    pub params: Vec<ParamInfo>,
    pub return_type: TypeSchema,
    /// Optional declared primary input type name (e.g., payload), if any
    pub input_type_name: Option<String>,
    /// Declared output type name; required by macros
    pub output_type_name: String,
    pub is_async: bool,
    pub is_mutation: bool,
}

/// Parameter information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamInfo {
    pub name: String,
    pub full_type: String,
    pub extractor: String,
    pub inner_type_schema: TypeSchema,
}

/// Error types for laz RPC operations
#[derive(Debug, Error)]
pub enum LazError {
    #[error("HTTP request failed: {0}")]
    RequestError(String),
    #[error("JSON parsing failed: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("Function not found: {0}")]
    FunctionNotFound(String),
    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),
    #[error("Server error: {0}")]
    ServerError(String),
    #[error("Type generation failed: {0}")]
    TypeGenerationError(String),
}

/// Helper to construct FunctionMetadata to avoid missing-field errors in macro sites
pub fn make_function_metadata(
    function_name: String,
    params: Vec<ParamInfo>,
    return_type: TypeSchema,
    input_type_name: Option<String>,
    output_type_name: String,
    is_async: bool,
    is_mutation: bool,
) -> FunctionMetadata {
    FunctionMetadata {
        function_name,
        params,
        return_type,
        input_type_name,
        output_type_name,
        is_async,
        is_mutation,
    }
}

/// Inventory entry that lazily constructs and exposes a type schema
pub struct TypeSchemaEntry {
    pub type_name: &'static str,
    pub getter: fn() -> &'static TypeSchema,
}

/// Inventory entry that lazily constructs and exposes function metadata
pub struct FunctionMetadataEntry {
    pub function_name: &'static str,
    pub getter: fn() -> &'static FunctionMetadata,
}

// Registry for schemas and metadata using inventory pattern
inventory::collect!(TypeSchema);
inventory::collect!(TypeSchemaEntry);
inventory::collect!(FunctionMetadata);
inventory::collect!(FunctionMetadataEntry);

/// Global registry for function metadata
use std::sync::{RwLock, OnceLock};
use std::collections::HashMap;

static FUNCTION_METADATA_REGISTRY: OnceLock<RwLock<HashMap<String, FunctionMetadata>>> =
    OnceLock::new();

/// Register function metadata in the global registry
pub fn register_function_metadata(metadata: FunctionMetadata) {
    let registry = FUNCTION_METADATA_REGISTRY.get_or_init(|| RwLock::new(HashMap::new()));
    let mut registry_guard = registry.write().unwrap();
    registry_guard.insert(metadata.function_name.clone(), metadata);
}

/// Get all registered function metadata
pub fn get_all_registered_functions() -> Vec<FunctionMetadata> {
    if let Some(registry) = FUNCTION_METADATA_REGISTRY.get() {
        let registry_guard = registry.read().unwrap();
        registry_guard.values().cloned().collect()
    } else {
        Vec::new()
    }
}

/// Get all collected type schemas
pub fn get_all_type_schemas() -> Vec<&'static TypeSchema> {
    let mut schemas: Vec<&'static TypeSchema> = inventory::iter::<TypeSchemaEntry>
        .into_iter()
        .map(|entry| (entry.getter)())
        .collect();

    schemas.extend(inventory::iter::<TypeSchema>);
    schemas
}

/// Get all collected function metadata
pub fn get_all_function_metadata() -> Vec<&'static FunctionMetadata> {
    let mut metadata: Vec<&'static FunctionMetadata> = inventory::iter::<FunctionMetadataEntry>
        .into_iter()
        .map(|entry| (entry.getter)())
        .collect();
    metadata.extend(inventory::iter::<FunctionMetadata>);
    metadata
}

/// Find a type schema by name
pub fn find_type_schema(type_name: &str) -> Option<&'static TypeSchema> {
    for entry in inventory::iter::<TypeSchemaEntry> {
        let schema = (entry.getter)();
        if entry.type_name == type_name {
            return Some(schema);
        }
        match schema {
            TypeSchema::Primitive(name) => {
                if name == type_name {
                    return Some(schema);
                }
            }
            TypeSchema::Struct(s) => {
                if s.type_name == type_name {
                    return Some(schema);
                }
            }
            TypeSchema::Enum(e) => {
                if e.type_name == type_name {
                    return Some(schema);
                }
            }
            TypeSchema::Opaque(name) => {
                if name == type_name {
                    return Some(schema);
                }
            }
            _ => continue,
        }
    }

    for schema in inventory::iter::<TypeSchema> {
        match schema {
            TypeSchema::Primitive(name) => {
                if name == type_name {
                    return Some(schema);
                }
            }
            TypeSchema::Struct(s) => {
                if s.type_name == type_name {
                    return Some(schema);
                }
            }
            TypeSchema::Enum(e) => {
                if e.type_name == type_name {
                    return Some(schema);
                }
            }
            TypeSchema::Opaque(name) => {
                if name == type_name {
                    return Some(schema);
                }
            }
            _ => continue,
        }
    }
    None
}

/// Endpoint discovery information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointDiscovery {
    pub uri: String,
    pub methods: Vec<String>,
}

/// Server address configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerAddr {
    pub ip: String,
    pub port: u16,
}

impl ServerAddr {
    pub fn base_url(&self) -> String {
        format!("http://{}:{}", self.ip, self.port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_addr_base_url() {
        let addr = ServerAddr {
            ip: "localhost".to_string(),
            port: 8080,
        };
        assert_eq!(addr.base_url(), "http://localhost:8080");
    }

    #[test]
    fn test_type_schema_creation() {
        let schema = TypeSchema::Primitive("String".to_string());
        assert!(matches!(schema, TypeSchema::Primitive(_)));
    }
}
