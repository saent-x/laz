use serde::Deserialize;
use std::collections::HashMap;

/// Schema information from server metadata
#[derive(Debug, Clone, Deserialize)]
pub struct TypeSchema {
    pub kind: String,
    pub value: serde_json::Value,
}

/// Function metadata with schema information
#[derive(Debug, Clone, Deserialize)]
pub struct FunctionMetadata {
    pub function_name: String,
    pub is_mutation: bool,
    pub is_async: bool,
    pub input_type_name: Option<String>,
    pub output_type_name: String,
    pub input_schema_json: Option<String>,
    pub output_schema_json: Option<String>,
}

/// Generated type information
#[derive(Debug, Clone)]
pub struct GeneratedType {
    pub name: String,
    pub definition: String,
    pub is_struct: bool,
    pub fields: Vec<TypeField>,
}

#[derive(Debug, Clone)]
pub struct TypeField {
    pub name: String,
    pub field_type: String,
    pub optional: bool,
}

/// Generate Rust types from server schema
pub fn generate_types_from_metadata(functions: &[FunctionMetadata]) -> Vec<GeneratedType> {
    let mut generated_types = Vec::new();
    let mut processed_types = HashMap::new();

    for func in functions {
        // Generate input type if present
        if let (Some(type_name), Some(schema_json)) = (&func.input_type_name, &func.input_schema_json) {
            if !processed_types.contains_key(type_name) {
                if let Ok(generated_type) = generate_type_from_schema(type_name, schema_json) {
                    processed_types.insert(type_name.clone(), true);
                    generated_types.push(generated_type);
                }
            }
        }

        // Generate output type
        if !func.output_type_name.is_empty() && !processed_types.contains_key(&func.output_type_name) {
            if let Some(schema_json) = &func.output_schema_json {
                if let Ok(generated_type) = generate_type_from_schema(&func.output_type_name, schema_json) {
                    processed_types.insert(func.output_type_name.clone(), true);
                    generated_types.push(generated_type);
                }
            } else {
                // For types without schema, generate a basic type
                let basic_type = generate_basic_type(&func.output_type_name);
                processed_types.insert(func.output_type_name.clone(), true);
                generated_types.push(basic_type);
            }
        }
    }

    generated_types
}

fn generate_type_from_schema(name: &str, schema_json: &str) -> Result<GeneratedType, Box<dyn std::error::Error>> {
    let schema: serde_json::Value = serde_json::from_str(schema_json)?;
    
    match schema.get("kind").and_then(|k| k.as_str()) {
        Some("Struct") => generate_struct_type(name, &schema),
        Some("Enum") => generate_enum_type(name, &schema),
        Some("Primitive") => Ok(generate_basic_type(name)),
        _ => Ok(generate_basic_type(name)),
    }
}

fn generate_struct_type(name: &str, schema: &serde_json::Value) -> Result<GeneratedType, Box<dyn std::error::Error>> {
    let value = schema.get("value").ok_or("Missing value in struct schema")?;
    let type_name = value.get("type_name").and_then(|n| n.as_str()).unwrap_or(name);
    
    let mut fields = Vec::new();
    if let Some(fields_array) = value.get("fields").and_then(|f| f.as_array()) {
        for field_value in fields_array {
            let field_name = field_value.get("field_name")
                .and_then(|n| n.as_str())
                .ok_or("Missing field_name")?;
            
            let field_type_info = field_value.get("field_type")
                .ok_or("Missing field_type")?;
            
            let field_type = get_field_type_string(field_type_info);
            let optional = field_value.get("optional").and_then(|o| o.as_bool()).unwrap_or(false);
            
            fields.push(TypeField {
                name: field_name.to_string(),
                field_type,
                optional,
            });
        }
    }

    let mut definition = format!("#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]\n");
    definition.push_str(&format!("pub struct {} {{\n", type_name));
    
    for field in &fields {
        if field.optional {
            definition.push_str(&format!("    pub {}: Option<{}>,\n", field.name, field.field_type));
        } else {
            definition.push_str(&format!("    pub {}: {},\n", field.name, field.field_type));
        }
    }
    
    definition.push_str("}\n");

    Ok(GeneratedType {
        name: type_name.to_string(),
        definition,
        is_struct: true,
        fields,
    })
}

fn generate_enum_type(name: &str, schema: &serde_json::Value) -> Result<GeneratedType, Box<dyn std::error::Error>> {
    let value = schema.get("value").ok_or("Missing value in enum schema")?;
    let type_name = value.get("type_name").and_then(|n| n.as_str()).unwrap_or(name);
    
    let mut definition = format!("#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]\n");
    definition.push_str(&format!("pub enum {} {{\n", type_name));
    
    if let Some(variants_array) = value.get("variants").and_then(|v| v.as_array()) {
        for variant_value in variants_array {
            let variant_name = variant_value.get("variant_name")
                .and_then(|n| n.as_str())
                .ok_or("Missing variant_name")?;
            
            definition.push_str(&format!("    {},\n", variant_name));
        }
    }
    
    definition.push_str("}\n");

    Ok(GeneratedType {
        name: type_name.to_string(),
        definition,
        is_struct: false,
        fields: Vec::new(),
    })
}

fn get_field_type_string(field_type_info: &serde_json::Value) -> String {
    match field_type_info.get("kind").and_then(|k| k.as_str()) {
        Some("Primitive") => {
            field_type_info.get("value")
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
                .to_string()
        },
        Some("Struct") => {
            field_type_info.get("value")
                .and_then(|v| v.get("type_name"))
                .and_then(|n| n.as_str())
                .unwrap_or("serde_json::Value")
                .to_string()
        },
        Some("Container") => {
            // Handle Vec<T>, Option<T>, etc.
            if let Some(container_type) = field_type_info.get("container_type").and_then(|c| c.as_str()) {
                if let Some(inner_type) = field_type_info.get("inner_type") {
                    let inner_type_str = get_field_type_string(inner_type);
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
        },
        _ => "serde_json::Value".to_string(),
    }
}

fn generate_basic_type(name: &str) -> GeneratedType {
    let definition = format!("#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]\npub struct {}(pub serde_json::Value);\n", name);
    
    GeneratedType {
        name: name.to_string(),
        definition,
        is_struct: true,
        fields: vec![TypeField {
            name: "value".to_string(),
            field_type: "serde_json::Value".to_string(),
            optional: false,
        }],
    }
}

/// Generate function signatures with proper types
pub fn generate_typed_function_signature(
    func_name: &str,
    is_mutation: bool,
    input_type_name: Option<&str>,
    output_type_name: &str,
) -> String {
    let input_param = if let Some(input_type) = input_type_name {
        format!("params: {}", input_type)
    } else {
        "".to_string()
    };

    let return_type = if output_type_name.is_empty() {
        "()".to_string()
    } else {
        output_type_name.to_string()
    };

    if is_mutation {
        if input_type_name.is_some() {
            format!(
                "pub async fn {}(&self, {}) -> Result<{}, ::laz_client::RpcClientError>",
                func_name, input_param, return_type
            )
        } else {
            format!(
                "pub async fn {}(&self) -> Result<{}, ::laz_client::RpcClientError>",
                func_name, return_type
            )
        }
    } else {
        if input_type_name.is_some() {
            format!(
                "pub async fn {}(&self, {}) -> Result<{}, ::laz_client::RpcClientError>",
                func_name, input_param, return_type
            )
        } else {
            format!(
                "pub async fn {}(&self) -> Result<{}, ::laz_client::RpcClientError>",
                func_name, return_type
            )
        }
    }
}

/// Generate function body with proper serialization
pub fn generate_typed_function_body(
    func_name: &str,
    input_type_name: Option<&str>,
    output_type_name: &str,
) -> String {
    let call_part = if let Some(_input_type) = input_type_name {
        format!("self.inner.call_function(\"{}\", Some(serde_json::to_value(&params)?)).await?", func_name)
    } else {
        format!("self.inner.call_function(\"{}\", None).await?", func_name)
    };

    let return_part = if output_type_name.is_empty() {
        format!("{};\n        Ok(())", call_part)
    } else {
        format!("let result = {};\n        serde_json::from_value(result).map_err(|e| ::laz_client::RpcClientError::JsonError(e))", call_part)
    };

    format!("    {}\n", return_part)
}