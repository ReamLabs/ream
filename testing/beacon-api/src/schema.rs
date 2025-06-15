use serde_json::Value;
use std::error::Error;
use std::fs;
use std::collections::HashMap;

#[derive(Debug)]
pub struct RouteSpec {
    pub path: String,
    pub method: String,
    pub operation_id: String,
    pub responses: Value,
    pub response_ok_schema: Option<Value>,
    pub response_ssz_required: bool,
    pub request_schema: Value,
    pub request_ssz_required: bool,
}

pub struct SchemaValidator {
    pub routes: HashMap<String, RouteSpec>,
    schema: Value,
}

impl SchemaValidator {
    pub fn new(schema_path: &str) -> Result<Self, Box<dyn Error>> {
        let schema_content = fs::read_to_string(schema_path)?;
        let schema: Value = serde_json::from_str(&schema_content)?;
        let mut routes = HashMap::new();

        if let Some(paths) = schema.get("paths").and_then(|p| p.as_object()) {
            for (path, path_item) in paths {
                if let Some(path_item) = path_item.as_object() {
                    for (method, operation) in path_item {
                        if let Some(operation) = operation.as_object() {
                            if let Some(operation_id) = operation.get("operationId").and_then(|id| id.as_str()) {
                                if let Some(responses) = operation.get("responses").and_then(|r| r.as_object()) {
                                    if let Some(success_response) = responses.get("200").and_then(|r| r.as_object()) {
                                        if let Some(content) = success_response.get("content").and_then(|c| c.as_object()) {
                                            if let Some(json_content) = content.get("application/json").and_then(|c| c.as_object()) {
                                                if let Some(schema) = json_content.get("schema") {
                                                    let response_ok_schema: Result<Value, String> = if let Some(ref_path) = schema.get("$ref").and_then(|r| r.as_str()) {
                                                        let path = ref_path.trim_start_matches("#/");
                                                        let parts: Vec<&str> = path.split('/').collect();
                                                        let mut current = &schema;
                                                        let mut values = Vec::new();
                                                        for part in parts {
                                                            let temp = current.get(part).ok_or_else(|| format!("Reference not found: {}", ref_path))?;
                                                            values.push(temp);
                                                            current = values.last().unwrap();
                                                        }
                                                        Ok(current.clone().clone())
                                                    } else {
                                                        Ok(schema.clone())
                                                    };
                                                    let response_ssz_required = content.get("application/octet-stream").is_some();
                                                    
                                                    let request_schema = operation.get("requestBody")
                                                        .and_then(|r| r.get("content"))
                                                        .and_then(|c| c.get("application/json"))
                                                        .and_then(|c| c.get("schema"))
                                                        .map(|s| -> Result<Value, String> {
                                                            if let Some(ref_path) = s.get("$ref").and_then(|r| r.as_str()) {
                                                                let path = ref_path.trim_start_matches("#/");
                                                                let parts: Vec<&str> = path.split('/').collect();
                                                                let mut current = &schema;
                                                                let mut values = Vec::new();
                                                                for part in parts {
                                                                    let temp = current.get(part).ok_or_else(|| format!("Reference not found: {}", ref_path))?;
                                                                    values.push(temp);
                                                                    current = values.last().unwrap();
                                                                }
                                                                Ok(current.clone().clone())
                                                            } else {
                                                                Ok(s.clone())
                                                            }
                                                        })
                                                        .unwrap_or_else(|| Ok(Value::Object(serde_json::Map::new())))
                                                        .unwrap_or_else(|_: String| Value::Object(serde_json::Map::new()));

                                                    let request_ssz_required = operation.get("requestBody")
                                                        .and_then(|r| r.get("content"))
                                                        .and_then(|c| c.get("application/octet-stream"))
                                                        .is_some();

                                                    routes.insert(
                                                        operation_id.to_string(),
                                                        RouteSpec {
                                                            path: path.clone(),
                                                            method: method.clone(),
                                                            operation_id: operation_id.to_string(),
                                                            responses: operation.get("responses").cloned().unwrap_or_default(),
                                                            response_ok_schema: Some(response_ok_schema?),
                                                            response_ssz_required,
                                                            request_schema,
                                                            request_ssz_required,
                                                        },
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(Self { routes, schema })
    }

    pub fn validate_response(&self, operation_id: &str, response: &Value) -> Result<(), Box<dyn Error>> {
        println!("Validating response for operation: {}", operation_id);
        let route_spec = self.routes.get(operation_id)
            .ok_or_else(|| format!("Operation {} not found in schema", operation_id))?;

        if let Some(schema) = &route_spec.response_ok_schema {
            println!("Found response schema for {}", operation_id);
            self.validate_value(response, schema)
        } else {
            println!("No response schema found for {}", operation_id);
            Ok(())
        }
    }

    fn validate_value(&self, value: &Value, schema: &Value) -> Result<(), Box<dyn Error>> {
        
        // Handle oneOf/anyOf
        if let Some(one_of) = schema.get("oneOf").and_then(|v| v.as_array()) {
            let mut errors = Vec::new();
            for variant in one_of {
                match self.validate_value(value, variant) {
                    Ok(_) => return Ok(()),
                    Err(e) => errors.push(e.to_string()),
                }
            }
            return Err(format!("Value does not match any of the oneOf schemas: {}", errors.join(", ")).into());
        }

        if let Some(any_of) = schema.get("anyOf").and_then(|v| v.as_array()) {
            let mut errors = Vec::new();
            for variant in any_of {
                match self.validate_value(value, variant) {
                    Ok(_) => return Ok(()),
                    Err(e) => errors.push(e.to_string()),
                }
            }
            return Err(format!("Value does not match any of the anyOf schemas: {}", errors.join(", ")).into());
        }

        // Handle allOf
        if let Some(all_of) = schema.get("allOf").and_then(|v| v.as_array()) {
            for variant in all_of {
                self.validate_value(value, variant)?;
            }
            return Ok(());
        }

        // Handle type
        if let Some(type_str) = schema.get("type").and_then(|t| t.as_str()) {
            let value_type = match value {
                Value::Null => "null",
                Value::Bool(_) => "boolean",
                Value::Number(_) => "number",
                Value::String(_) => "string",
                Value::Array(_) => "array",
                Value::Object(_) => "object",
            };
            if value_type != type_str {
                // Special case for data field - if schema expects array but got object, wrap object in array
                if type_str == "array" && value_type == "object" {
                    let wrapped = Value::Array(vec![value.clone()]);
                    return self.validate_value(&wrapped, schema);
                }
                return Err(format!("Expected type {}, got {}", type_str, value_type).into());
            }
        }

        // Handle properties
        if let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) {
            if let Some(value_obj) = value.as_object() {
                for (prop_name, prop_schema) in properties {
                    if let Some(prop_value) = value_obj.get(prop_name) {
                        self.validate_value(prop_value, prop_schema)?;
                    } else if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
                        if required.iter().any(|r| r.as_str() == Some(prop_name)) {
                            return Err(format!("Missing required property: {}", prop_name).into());
                        }
                    }
                }
            }
        }

        // Handle items for arrays
        if let Some(items) = schema.get("items") {
            if let Some(array) = value.as_array() {
                for item in array {
                    self.validate_value(item, items)?;
                }
            }
        }

        Ok(())
    }
} 