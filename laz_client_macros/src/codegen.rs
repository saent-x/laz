use std::collections::HashMap;
use std::fs;

/// Function metadata from the server
#[derive(Debug, serde::Deserialize)]
struct FunctionMetadata {
    function_name: String,
    is_mutation: bool,
    is_async: bool,
    input_type_name: Option<String>,
    output_type_name: String,
    params: Vec<serde_json::Value>,
    input_schema_json: Option<String>,
    output_schema_json: Option<String>,
}

/// Endpoint discovery data
#[derive(Debug, serde::Deserialize)]
struct EndpointDiscovery {
    uri: String,
    methods: Vec<String>,
}

/// Complete metadata response from server
#[derive(Debug, serde::Deserialize)]
struct MetadataResponse {
    total_functions: usize,
    functions: Vec<serde_json::Value>,
    endpoints_discovery: Vec<serde_json::Value>,
    total_endpoints: usize,
}

/// Generate RPC client code based on server metadata
pub fn generate_client_code(server_url: &str) -> Result<String, Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=build.rs");
    
    // Fetch metadata from server
    let metadata_url = format!("{}/_laz/metadata", server_url);
    println!("Fetching RPC metadata from: {}", metadata_url);
    
    let response = reqwest::blocking::get(&metadata_url)
        .map_err(|e| format!("Failed to fetch RPC metadata: {}", e))?;
    
    if !response.status().is_success() {
        return Err(format!("Server returned error status: {}", response.status()).into());
    }
    
    let response_text = response.text()
        .map_err(|e| format!("Failed to read response text: {}", e))?;
    
    let metadata: MetadataResponse = serde_json::from_str(&response_text)
        .map_err(|e| format!("Failed to parse metadata JSON: {}", e))?;
    
    println!("Received metadata for {} functions and {} endpoints", 
             metadata.total_functions, metadata.total_endpoints);
    
    // Parse functions
    let mut functions = Vec::new();
    for func_value in metadata.functions {
        let function_name = func_value["function_name"]
            .as_str()
            .ok_or("Missing function_name")?
            .to_string();
        
        let is_mutation = func_value["is_mutation"].as_bool().unwrap_or(false);
        let is_async = func_value["is_async"].as_bool().unwrap_or(false);
        let input_type_name = func_value["input_type_name"].as_str().map(String::from);
        let output_type_name = func_value["output_type_name"]
            .as_str()
            .ok_or("Missing output_type_name")?
            .to_string();
        
        functions.push(FunctionMetadata {
            function_name,
            is_mutation,
            is_async,
            input_type_name,
            output_type_name,
            params: vec![],
            input_schema_json: None,
            output_schema_json: None,
        });
    }
    
    // Parse endpoints
    let mut endpoints = HashMap::new();
    for endpoint_value in metadata.endpoints_discovery {
        let uri = endpoint_value["uri"]
            .as_str()
            .ok_or("Missing endpoint uri")?
            .to_string();
        
        let methods = endpoint_value["methods"]
            .as_array()
            .ok_or("Missing endpoint methods")?
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        
        endpoints.insert(uri.clone(), methods);
    }
    
    // Generate the client code
    generate_client_impl(&functions, &endpoints, server_url)
}

/// Generate the actual client implementation code
fn generate_client_impl(
    functions: &[FunctionMetadata], 
    endpoints: &HashMap<String, Vec<String>>,
    server_url: &str
) -> Result<String, Box<dyn std::error::Error>> {
    let mut code = String::new();
    
    // Add imports and struct definition
    code.push_str(&format!(r#"
/// Auto-generated RPC client for server at: {}
/// Generated at build time from server metadata
pub struct GeneratedRpcClient {{
    inner: crate::LocoClient,
}}

impl GeneratedRpcClient {{
    /// Initialize the generated RPC client
    pub async fn init(server_addr: crate::ServerAddr) -> Result<Self, crate::RpcClientError> {{
        let client = crate::LocoClient::init(server_addr).await?;
        Ok(Self {{ inner: client }})
    }}
    
    /// Get the underlying LocoClient for advanced usage
    pub fn inner(&self) -> &crate::LocoClient {{
        &self.inner
    }}
    
    /// Get the server address
    pub fn server_addr(&self) -> &crate::ServerAddr {{
        &self.inner.server_addr
    }}

"#, server_url));
    
    // Generate function implementations
    for func in functions {
        let func_name = &func.function_name;
        let endpoint = find_endpoint_for_function(func_name, endpoints);
        
        if let Some(endpoint) = endpoint {
            let func_impl = generate_function_impl(func, &endpoint);
            code.push_str(&func_impl);
            code.push('\n');
        } else {
            eprintln!("Warning: No endpoint found for function '{}'", func_name);
        }
    }
    
    code.push_str("}\n");
    Ok(code)
}

/// Generate implementation for a single function
fn generate_function_impl(func: &FunctionMetadata, endpoint: &str) -> String {
    let func_name = &func.function_name;
    let _output_type = &func.output_type_name;
    
    // For now, use serde_json::Value as the return type
    // In a more advanced implementation, we could generate proper types
    
    if func.is_mutation {
        if func.input_type_name.is_some() {
            format!(r#"
    /// Call the {} function with parameters
    /// Endpoint: {}
    pub async fn {}(&self, params: serde_json::Value) -> Result<serde_json::Value, crate::RpcClientError> {{
        self.inner.call_function("{}", Some(params)).await
    }}"#, func_name, endpoint, func_name, func_name)
        } else {
            format!(r#"
    /// Call the {} function
    /// Endpoint: {}
    pub async fn {}(&self) -> Result<serde_json::Value, crate::RpcClientError> {{
        self.inner.call_function("{}", None).await
    }}"#, func_name, endpoint, func_name, func_name)
        }
    } else {
        if func.input_type_name.is_some() {
            format!(r#"
    /// Call the {} function with parameters
    /// Endpoint: {}
    pub async fn {}(&self, params: serde_json::Value) -> Result<serde_json::Value, crate::RpcClientError> {{
        self.inner.call_function("{}", Some(params)).await
    }}"#, func_name, endpoint, func_name, func_name)
        } else {
            format!(r#"
    /// Call the {} function
    /// Endpoint: {}
    pub async fn {}(&self) -> Result<serde_json::Value, crate::RpcClientError> {{
        self.inner.call_function("{}", None).await
    }}"#, func_name, endpoint, func_name, func_name)
        }
    }
}

/// Find the best matching endpoint for a function
fn find_endpoint_for_function(function_name: &str, endpoints: &HashMap<String, Vec<String>>) -> Option<String> {
    // Try exact matches first
    for (uri, _methods) in endpoints {
        if uri.contains(function_name) || uri.contains(&function_name.replace('_', "-")) {
            return Some(uri.clone());
        }
    }
    
    // Try common patterns
    let patterns = vec![
        format!("/api/{}", function_name.replace('_', "-")),
        format!("/api/auth/{}", function_name.replace('_', "-")),
        format!("/auth/{}", function_name.replace('_', "-")),
    ];
    
    for pattern in patterns {
        if endpoints.contains_key(&pattern) {
            return Some(pattern);
        }
    }
    
    // Return first endpoint as fallback
    endpoints.keys().next().cloned()
}

/// Write generated code to a file
pub fn write_generated_code(code: &str, out_dir: &str) -> Result<String, Box<dyn std::error::Error>> {
    let out_path = std::path::Path::new(out_dir).join("generated_rpc_client.rs");
    fs::write(&out_path, code)?;
    Ok(out_path.to_string_lossy().to_string())
}