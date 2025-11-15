use reqwest::blocking::Client;
use serde_json::Value;
use std::collections::HashMap;
use std::error::Error;

pub fn fetch_metadata_json(server_url: &str) -> Result<String, Box<dyn Error>> {
    let metadata_url = format!("{}/_laz/metadata", server_url.trim_end_matches('/'));
    let client = Client::new();
    let response = client.get(&metadata_url).send()?;

    if !response.status().is_success() {
        return Err(format!("Failed to fetch metadata: HTTP {}", response.status()).into());
    }

    Ok(response.text()?)
}

pub fn generate_client_code_from_metadata_json(
    server_url: &str,
    metadata_json: &str,
) -> Result<String, Box<dyn Error>> {
    let metadata: Value = serde_json::from_str(metadata_json)?;
    let functions = metadata["functions"]
        .as_array()
        .ok_or("No functions found in metadata")?
        .to_vec();
    let endpoints = metadata["endpoints_discovery"]
        .as_array()
        .cloned()
        .unwrap_or_else(Vec::new);

    generate_dynamic_typed_client(&functions, &endpoints, server_url)
}

pub fn generate_client_code_from_server(
    server_url: &str,
) -> Result<(String, String), Box<dyn Error>> {
    let metadata_json = fetch_metadata_json(server_url)?;
    let generated_code = generate_client_code_from_metadata_json(server_url, &metadata_json)?;
    Ok((generated_code, metadata_json))
}

fn generate_dynamic_typed_client(
    functions: &[Value],
    endpoints: &[Value],
    server_url: &str,
) -> Result<String, Box<dyn Error>> {
    let mut code = String::new();
    let mut types = HashMap::new();
    let endpoint_map = build_endpoint_map(endpoints);

    for func in functions {
        if let (Some(_func_name), Some(output_type)) = (
            func["function_name"].as_str(),
            func["output_type_name"].as_str(),
        ) {
            if let Some(input_type) = func["input_type_name"].as_str() {
                if !input_type.is_empty() && !types.contains_key(input_type) {
                    let type_def =
                        generate_type_from_schema(input_type, func["input_schema_json"].as_str());
                    types.insert(input_type.to_string(), type_def);
                }
            }

            if !output_type.is_empty() && !types.contains_key(output_type) {
                let type_def =
                    generate_type_from_schema(output_type, func["output_schema_json"].as_str());
                types.insert(output_type.to_string(), type_def);
            }
        }
    }

    let mut type_definitions = String::new();
    for type_def in types.values() {
        if !type_def.trim().is_empty() {
            type_definitions.push_str(type_def);
            type_definitions.push_str("\n\n");
        }
    }

    code.push_str(&format!(
        r#"
/// Auto-generated type-safe RPC client for server at: {}
/// Generated at build time from actual server metadata
/// Found {} functions and {} unique types

// Define all generated types first
{}

/// Auto-generated type-safe RPC client
pub struct GeneratedRpcClient {{
    inner: ::laz_client::LocoClient,
}}

impl GeneratedRpcClient {{
    pub async fn init(server_addr: ::laz_client::ServerAddr) -> Result<Self, ::laz_client::RpcClientError> {{
        let client = ::laz_client::LocoClient::init(server_addr).await?;
        Ok(Self {{ inner: client }})
    }}

    pub fn inner(&self) -> &::laz_client::LocoClient {{
        &self.inner
    }}

    pub fn server_addr(&self) -> &::laz_client::ServerAddr {{
        &self.inner.server_addr
    }}

"#,
        server_url,
        functions.len(),
        types.len(),
        type_definitions
    ));

    for func in functions {
        if let (Some(func_name), Some(is_mutation), Some(output_type)) = (
            func["function_name"].as_str(),
            func["is_mutation"].as_bool(),
            func["output_type_name"].as_str(),
        ) {
            let input_type = func["input_type_name"].as_str();
            let endpoint_hint = find_endpoint_for_function(func_name, &endpoint_map)
                .unwrap_or_else(|| format!("/{}", func_name));
            let func_impl = generate_typed_function_impl(
                func_name,
                is_mutation,
                input_type,
                output_type,
                &endpoint_hint,
            );
            code.push_str(&func_impl);
            code.push('\n');
        }
    }

    code.push_str("\n}\n");
    Ok(code)
}

fn generate_type_from_schema(type_name: &str, schema_json: Option<&str>) -> String {
    // Don't generate custom types for primitive types that conflict with Rust built-ins
    if matches!(type_name, "String" | "i32" | "i64" | "bool" | "f32" | "f64") {
        return String::new();
    }

    if let Some(schema) = schema_json {
        if let Ok(schema_value) = serde_json::from_str::<Value>(schema) {
            if let Some(kind) = schema_value.get("kind").and_then(|k| k.as_str()) {
                match kind {
                    "Struct" => return generate_struct_type_from_schema(type_name, &schema_value),
                    "Enum" => return generate_enum_type_from_schema(type_name, &schema_value),
                    "Primitive" => {
                        return generate_primitive_type_from_schema(type_name, &schema_value)
                    }
                    _ => {}
                }
            }
        }
    }

    generate_basic_type(type_name)
}

fn generate_struct_type_from_schema(name: &str, schema: &Value) -> String {
    let mut code = format!(
        "#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]\npub struct {} {{\n",
        name
    );

    if let Some(value) = schema.get("value") {
        if let Some(fields) = value.get("fields").and_then(|f| f.as_array()) {
            for field in fields {
                if let (Some(field_name), Some(field_type_info)) = (
                    field.get("field_name").and_then(|n| n.as_str()),
                    field.get("field_type"),
                ) {
                    let field_type = get_rust_type_from_schema(field_type_info);
                    let optional = field
                        .get("optional")
                        .and_then(|o| o.as_bool())
                        .unwrap_or(false);

                    if optional {
                        code.push_str(&format!(
                            "    pub {}: Option<{}>,\n",
                            field_name, field_type
                        ));
                    } else {
                        code.push_str(&format!("    pub {}: {},\n", field_name, field_type));
                    }
                }
            }
        }
    }

    code.push_str("}\n");
    code
}

fn generate_enum_type_from_schema(name: &str, schema: &Value) -> String {
    let mut code = format!(
        "#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]\npub enum {} {{\n",
        name
    );

    if let Some(value) = schema.get("value") {
        if let Some(variants) = value.get("variants").and_then(|v| v.as_array()) {
            for variant in variants {
                if let Some(variant_name) = variant.get("variant_name").and_then(|n| n.as_str()) {
                    code.push_str(&format!("    {},\n", variant_name));
                }
            }
        }
    }

    code.push_str("}\n");
    code
}

fn generate_primitive_type_from_schema(name: &str, schema: &Value) -> String {
    if let Some(value) = schema.get("value").and_then(|v| v.as_str()) {
        match value {
            v if v == name => String::new(),
            "String" => {
                if name == "String" {
                    String::new()
                } else {
                    format!(
                        "#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]\npub struct {}(pub String);\n",
                        name
                    )
                }
            }
            "i32" => format!(
                "#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]\npub struct {}(pub i32);\n",
                name
            ),
            "i64" => format!(
                "#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]\npub struct {}(pub i64);\n",
                name
            ),
            "bool" => format!(
                "#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]\npub struct {}(pub bool);\n",
                name
            ),
            _ => generate_basic_type(name),
        }
    } else {
        generate_basic_type(name)
    }
}

fn get_rust_type_from_schema(field_type_info: &Value) -> String {
    match field_type_info.get("kind").and_then(|k| k.as_str()) {
        Some("Primitive") => field_type_info
            .get("value")
            .and_then(|v| v.as_str())
            .map(|s| match s {
                "String" => "String",
                "i32" => "i32",
                "i64" => "i64",
                "bool" => "bool",
                "f32" => "f32",
                "f64" => "f64",
                _ => "serde_json::Value",
            })
            .unwrap_or("serde_json::Value")
            .to_string(),
        Some("Struct") => field_type_info
            .get("value")
            .and_then(|v| v.get("type_name"))
            .and_then(|n| n.as_str())
            .unwrap_or("serde_json::Value")
            .to_string(),
        Some("Container") => {
            if let Some(container_type) = field_type_info
                .get("container_type")
                .and_then(|c| c.as_str())
            {
                if let Some(inner_type) = field_type_info.get("inner_type") {
                    let inner_type_str = get_rust_type_from_schema(inner_type);
                    match container_type {
                        "Vec" => format!("Vec<{}>", inner_type_str),
                        "Option" => format!("Option<{}>", inner_type_str),
                        _ => "serde_json::Value".to_string(),
                    }
                } else {
                    "serde_json::Value".to_string()
                }
            } else {
                "serde_json::Value".to_string()
            }
        }
        _ => "serde_json::Value".to_string(),
    }
}

fn generate_basic_type(name: &str) -> String {
    format!(
        "#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]\npub struct {}(pub serde_json::Value);\n",
        name
    )
}

fn generate_typed_function_impl(
    func_name: &str,
    is_mutation: bool,
    input_type_name: Option<&str>,
    output_type_name: &str,
    endpoint: &str,
) -> String {
    let output_type = if output_type_name.trim().is_empty() {
        "()"
    } else {
        output_type_name
    };

    let signature = if let Some(input_type) = input_type_name {
        if !input_type.is_empty() {
            let input_type_rust = match input_type {
                "String" => "String",
                "i32" => "i32",
                "i64" => "i64",
                "bool" => "bool",
                "f32" => "f32",
                "f64" => "f64",
                _ => input_type,
            };
            format!(
                "    pub async fn {}(&self, params: {}) -> Result<{}, ::laz_client::RpcClientError>",
                func_name, input_type_rust, output_type
            )
        } else {
            format!(
                "    pub async fn {}(&self) -> Result<{}, ::laz_client::RpcClientError>",
                func_name, output_type
            )
        }
    } else {
        format!(
            "    pub async fn {}(&self) -> Result<{}, ::laz_client::RpcClientError>",
            func_name, output_type
        )
    };

    let payload = if let Some(input_type) = input_type_name {
        if !input_type.is_empty() {
            "Some(serde_json::to_value(&params)?)"
        } else {
            "None"
        }
    } else {
        "None"
    };

    let call_expr = format!(
        "self.inner.call_endpoint(\"{}\", {}, {}).await?",
        endpoint, is_mutation, payload
    );

    let body = if output_type == "()" {
        format!("        {};\n        Ok(())", call_expr)
    } else {
        format!(
            "        let value = {};\n        serde_json::from_value(value).map_err(|e| ::laz_client::RpcClientError::JsonError(e))",
            call_expr
        )
    };

    format!(
        "    /// Auto-generated wrapper for `{}` hitting `{}`\n{}\n    {{\n{}\n    }}\n",
        func_name, endpoint, signature, body
    )
}

fn build_endpoint_map(values: &[Value]) -> HashMap<String, Vec<String>> {
    let mut map = HashMap::new();
    for entry in values {
        if let Some(uri) = entry.get("uri").and_then(|v| v.as_str()) {
            let methods = entry
                .get("methods")
                .and_then(|m| m.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            map.insert(uri.to_string(), methods);
        }
    }
    map
}

fn find_endpoint_for_function(
    function_name: &str,
    endpoints: &HashMap<String, Vec<String>>,
) -> Option<String> {
    for uri in endpoints.keys() {
        if uri.contains(function_name) || uri.contains(&function_name.replace('_', "-")) {
            return Some(uri.clone());
        }
    }

    let patterns = [
        format!("/api/{}", function_name.replace('_', "-")),
        format!("/api/auth/{}", function_name.replace('_', "-")),
        format!("/auth/{}", function_name.replace('_', "-")),
    ];

    for pattern in patterns {
        if endpoints.contains_key(&pattern) {
            return Some(pattern);
        }
    }

    endpoints.keys().next().cloned()
}
