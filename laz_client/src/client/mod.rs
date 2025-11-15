use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;
use tracing::{debug, info};

#[derive(Debug, Error)]
pub enum RpcClientError {
    #[error("HTTP request failed: {0}")]
    RequestError(#[from] reqwest::Error),
    #[error("JSON parsing failed: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("Function not found: {0}")]
    FunctionNotFound(String),
    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),
    #[error("Server error: {0}")]
    ServerError(String),
}

#[derive(Debug, Clone)]
pub struct ServerAddr {
    pub ip: String,
    pub port: usize,
}

impl ServerAddr {
    pub fn base_url(&self) -> String {
        format!("http://{}:{}", self.ip, self.port)
    }
}

#[derive(Debug, Clone)]
pub struct RpcFunction {
    pub name: String,
    pub is_mutation: bool,
    pub is_async: bool,
    pub input_type_name: Option<String>,
    pub output_type_name: String,
    pub params: Vec<Value>, // Store as JSON Value for now
    pub input_schema_json: Option<String>,
    pub output_schema_json: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EndpointDiscovery {
    pub uri: String,
    pub methods: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct LocoClient {
    pub server_addr: ServerAddr,
    http_client: Client,
    functions: HashMap<String, RpcFunction>,
    endpoints_discovery: Vec<EndpointDiscovery>,
}

#[derive(Debug, Deserialize)]
struct MetadataResponse {
    total_functions: usize,
    functions: Vec<Value>,
    endpoints_discovery: Vec<Value>,
    total_endpoints: usize,
}

impl LocoClient {
    /// Initialize the LocoClient by fetching metadata from the server
    /// # Example
    /// ```rust,no_run
    /// use laz_client::{LocoClient, ServerAddr};
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let rpc_client = LocoClient::init(ServerAddr{
    ///         ip: "localhost".to_string(),
    ///         port: 5150
    ///     }).await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn init(server_addr: ServerAddr) -> Result<Self, RpcClientError> {
        let http_client = Client::new();
        let mut client = Self {
            server_addr: server_addr.clone(),
            http_client,
            functions: HashMap::new(),
            endpoints_discovery: Vec::new(),
        };

        // Fetch metadata from server
        match client.fetch_metadata().await {
            Ok(_) => {
                tracing::info!("Successfully loaded RPC metadata from server");
            }
            Err(e) => {
                tracing::warn!("Failed to fetch RPC metadata: {}. Using basic client.", e);
                // Continue with empty metadata - client will still work for basic calls
            }
        }

        Ok(client)
    }

    /// Fetch metadata from the server's _laz/metadata endpoint
    async fn fetch_metadata(&mut self) -> Result<(), RpcClientError> {
        let metadata_url = format!("{}/_laz/metadata", self.server_addr.base_url());
        info!("Fetching RPC metadata from: {}", metadata_url);

        let response = self.http_client.get(&metadata_url).send().await?;

        if !response.status().is_success() {
            return Err(RpcClientError::ServerError(format!(
                "Failed to fetch metadata: HTTP {}",
                response.status()
            )));
        }

        let response_text = response.text().await?;
        debug!(
            "Raw metadata response length: {} bytes",
            response_text.len()
        );
        debug!(
            "Raw metadata response (first 500 chars): {}",
            &response_text[..response_text.len().min(500)]
        );

        let metadata_response: MetadataResponse = serde_json::from_str(&response_text)
            .map_err(|e| {
                RpcClientError::ServerError(format!(
                    "Failed to parse metadata JSON: {}. Response length: {} bytes, first 300 chars: {}",
                    e,
                    response_text.len(),
                    response_text.chars().take(300).collect::<String>()
                ))
            })?;

        debug!(
            "Received metadata for {} functions and {} endpoints",
            metadata_response.total_functions, metadata_response.total_endpoints
        );

        // Parse and store function metadata
        for func_value in metadata_response.functions {
            let function_name = func_value["function_name"]
                .as_str()
                .ok_or_else(|| {
                    RpcClientError::InvalidParameter("Missing function_name".to_string())
                })?
                .to_string();

            let is_mutation = func_value["is_mutation"].as_bool().unwrap_or(false);
            let is_async = func_value["is_async"].as_bool().unwrap_or(false);
            let input_type_name = func_value["input_type_name"].as_str().map(String::from);
            let output_type_name = func_value["output_type_name"]
                .as_str()
                .ok_or_else(|| {
                    RpcClientError::InvalidParameter("Missing output_type_name".to_string())
                })?
                .to_string();

            // Parse input schema if available - store as JSON string for now
            let input_schema_json = func_value["input_schema_json"].as_str().map(String::from);

            // Parse output schema - store as JSON string for now
            let output_schema_json = func_value["output_schema_json"].as_str().map(String::from);

            // Parse parameters - store as JSON value for now
            let params_value = func_value["params"].clone();

            let rpc_function = RpcFunction {
                name: function_name.clone(),
                is_mutation,
                is_async,
                input_type_name: input_type_name.clone(),
                output_type_name: output_type_name.clone(),
                params: vec![params_value], // Store the JSON value
                input_schema_json,
                output_schema_json,
            };

            self.functions.insert(function_name, rpc_function);
        }

        // Parse and store endpoints discovery
        for endpoint_value in metadata_response.endpoints_discovery {
            let uri = endpoint_value["uri"]
                .as_str()
                .ok_or_else(|| {
                    RpcClientError::InvalidParameter("Missing endpoint uri".to_string())
                })?
                .to_string();

            let methods = endpoint_value["methods"]
                .as_array()
                .ok_or_else(|| {
                    RpcClientError::InvalidParameter("Missing endpoint methods".to_string())
                })?
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();

            let endpoint_discovery = EndpointDiscovery { uri, methods };
            self.endpoints_discovery.push(endpoint_discovery);
        }

        info!(
            "Successfully loaded {} RPC functions and {} endpoints",
            self.functions.len(),
            self.endpoints_discovery.len()
        );
        Ok(())
    }

    /// Get a list of available function names
    pub fn get_function_names(&self) -> Vec<String> {
        self.functions.keys().cloned().collect()
    }

    /// Get the endpoints discovery data
    pub fn get_endpoints_discovery(&self) -> &[EndpointDiscovery] {
        &self.endpoints_discovery
    }

    /// Get metadata for a specific function
    pub fn get_function_metadata(&self, function_name: &str) -> Option<&RpcFunction> {
        self.functions.get(function_name)
    }

    /// Call an RPC function by name with parameters using dynamic endpoint discovery
    pub async fn call_function(
        &self,
        function_name: &str,
        params: Option<Value>,
    ) -> Result<Value, RpcClientError> {
        let function = self
            .functions
            .get(function_name)
            .ok_or_else(|| RpcClientError::FunctionNotFound(function_name.to_string()))?;

        let endpoint = self
            .find_endpoint_for_function(function_name)
            .ok_or_else(|| {
                RpcClientError::FunctionNotFound(format!(
                    "No endpoint found for function: {}",
                    function_name
                ))
            })?;
        self.call_endpoint(&endpoint, function.is_mutation, params)
            .await
    }

    /// Call a specific endpoint directly, bypassing endpoint discovery
    pub async fn call_endpoint(
        &self,
        endpoint: &str,
        is_mutation: bool,
        params: Option<Value>,
    ) -> Result<Value, RpcClientError> {
        let temp_endpoint = format!("/api{}", endpoint); // TODO: temporary url until I figure out how to automatically get the url
        let url = format!("{}{}", self.server_addr.base_url(), temp_endpoint);
        debug!("Calling RPC endpoint: {} (mutation = {})", url, is_mutation);
        eprintln!("Calling RPC endpoint: {} (mutation = {})", url, is_mutation);

        let response = if is_mutation {
            let mut request = self.http_client.post(&url);
            if let Some(params) = params {
                request = request.json(&params);
            }
            request.send().await?
        } else {
            let mut request = self.http_client.get(&url);
            if let Some(Value::Object(obj)) = params {
                let query_pairs: Vec<(String, String)> = obj
                    .into_iter()
                    .map(|(k, v)| (k, stringify_value(&v)))
                    .collect();
                if !query_pairs.is_empty() {
                    request = request.query(&query_pairs);
                }
            }
            request.send().await?
        };

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(RpcClientError::ServerError(format!(
                "Endpoint {} failed with status {}: {}",
                endpoint, status, error_text
            )));
        }

        let r = response.json::<Value>().await.map_err(RpcClientError::from);

        eprintln!("Response: {:#?}", r);
        r
    }

    /// Find the HTTP endpoint for a function using dynamic endpoint discovery
    fn find_endpoint_for_function(&self, function_name: &str) -> Option<String> {
        // Try to find a matching endpoint based on function name
        for endpoint in &self.endpoints_discovery {
            let uri = &endpoint.uri;
            println!("URI {}", uri);

            // Check if function name appears in the URI
            if uri.contains(function_name)
                || uri.contains(&function_name.replace('_', "-"))
                || uri.contains(&function_name.to_lowercase())
            {
                return Some(uri.clone());
            }

            // Check common patterns
            let patterns = vec![
                format!("/api/{}", function_name.replace('_', "-")),
                format!("/api/auth/{}", function_name.replace('_', "-")),
                format!("/{}/{}", "auth", function_name.replace('_', "-")),
            ];

            for pattern in patterns {
                if uri == &pattern {
                    return Some(uri.clone());
                }
            }
        }

        // Fallback: generate common patterns if no match found
        let fallback_patterns = vec![
            format!("/api/{}", function_name.replace('_', "-")),
            format!("/api/auth/{}", function_name.replace('_', "-")),
        ];

        for pattern in fallback_patterns {
            if self.endpoints_discovery.iter().any(|e| e.uri == pattern) {
                return Some(pattern);
            }
        }

        None
    }

    /// Helper method to call a function with typed input parameters
    pub async fn call_with_input<T: Serialize>(
        &self,
        function_name: &str,
        input: &T,
    ) -> Result<Value, RpcClientError> {
        let params = serde_json::to_value(input)?;
        self.call_function(function_name, Some(params)).await
    }
}

fn stringify_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_addr_base_url() {
        let addr = ServerAddr {
            ip: "localhost".to_string(),
            port: 5150,
        };
        assert_eq!(addr.base_url(), "http://localhost:5150");
    }

    #[test]
    fn test_function_metadata_storage() {
        let function = RpcFunction {
            name: "test_function".to_string(),
            is_mutation: true,
            is_async: false,
            input_type_name: Some("TestInput".to_string()),
            output_type_name: "TestOutput".to_string(),
            params: vec![],
            input_schema_json: None,
            output_schema_json: Some(r#"{"kind": "Primitive", "value": "String"}"#.to_string()),
        };

        let mut functions = HashMap::new();
        functions.insert("test_function".to_string(), function.clone());

        let client = LocoClient {
            server_addr: ServerAddr {
                ip: "localhost".to_string(),
                port: 5150,
            },
            http_client: Client::new(),
            functions,
            endpoints_discovery: Vec::new(),
        };

        assert!(client.get_function_metadata("test_function").is_some());
        assert_eq!(client.get_function_names().len(), 1);
    }

    #[test]
    fn test_endpoints_discovery() {
        let endpoint1 = EndpointDiscovery {
            uri: "/api/auth/login".to_string(),
            methods: vec!["POST".to_string()],
        };
        let endpoint2 = EndpointDiscovery {
            uri: "/api/auth/hello".to_string(),
            methods: vec!["GET".to_string()],
        };

        let endpoints_discovery = vec![endpoint1.clone(), endpoint2.clone()];

        let client = LocoClient {
            server_addr: ServerAddr {
                ip: "localhost".to_string(),
                port: 8080,
            },
            http_client: Client::new(),
            functions: HashMap::new(),
            endpoints_discovery: endpoints_discovery.clone(),
        };

        let discovered_endpoints = client.get_endpoints_discovery();
        assert_eq!(discovered_endpoints.len(), 2);
        assert_eq!(discovered_endpoints[0].uri, "/api/auth/login");
        assert_eq!(discovered_endpoints[0].methods, vec!["POST"]);
        assert_eq!(discovered_endpoints[1].uri, "/api/auth/hello");
        assert_eq!(discovered_endpoints[1].methods, vec!["GET"]);
    }

    #[test]
    fn test_metadata_response_deserialization() {
        let json_response = r#"
        {
            "total_functions": 2,
            "functions": [
                {
                    "function_name": "test_func1",
                    "is_mutation": true,
                    "is_async": false,
                    "input_type_name": "Input1",
                    "output_type_name": "Output1",
                    "params": [],
                    "input_schema_json": null,
                    "output_schema_json": null
                }
            ],
            "endpoints_discovery": [
                {
                    "uri": "/api/test",
                    "methods": ["GET", "POST"]
                }
            ],
            "total_endpoints": 1
        }
        "#;

        let metadata: MetadataResponse = serde_json::from_str(json_response).unwrap();
        assert_eq!(metadata.total_functions, 2);
        assert_eq!(metadata.functions.len(), 1);
        assert_eq!(metadata.total_endpoints, 1);
        assert_eq!(metadata.endpoints_discovery.len(), 1);

        let endpoint = &metadata.endpoints_discovery[0];
        assert_eq!(endpoint["uri"], "/api/test");
        assert_eq!(endpoint["methods"].as_array().unwrap().len(), 2);
    }
}
