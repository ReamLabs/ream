use serde_json::Value;
use std::error::Error;
use jsonschema::{JSONSchema, ValidationError};
use std::fmt;
use serde_json::json;

#[derive(Debug)]
pub struct SchemaValidationError {
    pub message: String,
}

impl fmt::Display for SchemaValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for SchemaValidationError {}

impl From<ValidationError<'_>> for SchemaValidationError {
    fn from(error: ValidationError<'_>) -> Self {
        SchemaValidationError {
            message: error.to_string(),
        }
    }
}

#[derive(Clone)]
pub struct RouteSpec {
    pub path: String,
    pub method: String,
    pub base_url: String,
    pub response_schema: Option<Value>,
    pub request_schema: Option<Value>,
}

pub struct SchemaValidator {
    schema: Value,
    route_specs: HashMap<String, RouteSpec>,
}

impl SchemaValidator {
    pub fn new(schema: Value) -> Result<Self, Box<dyn Error>> {
        let mut validator = Self {
            schema,
            route_specs: HashMap::new(),
        };
        validator.init_route_specs()?;
        Ok(validator)
    }

    fn init_route_specs(&mut self) -> Result<(), Box<dyn Error>> {
        let paths = self.schema.get("paths")
            .ok_or_else(|| SchemaValidationError { message: "No paths found in schema".to_string() })?;

        for (path, methods) in paths.as_object()
            .ok_or_else(|| SchemaValidationError { message: "Paths must be an object".to_string() })?
        {
            for (method, spec) in methods.as_object()
                .ok_or_else(|| SchemaValidationError { message: "Methods must be an object".to_string() })?
            {
                let operation_id = spec.get("operationId")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| SchemaValidationError { 
                        message: format!("No operationId found for {} {}", method, path) 
                    })?;

                // Get response schema
                let response_schema = spec.get("responses")
                    .and_then(|r| r.get("200"))
                    .and_then(|r| r.get("content"))
                    .and_then(|c| c.get("application/json"))
                    .and_then(|c| c.get("schema"))
                    .cloned();

                // Get request schema
                let request_schema = spec.get("requestBody")
                    .and_then(|r| r.get("content"))
                    .and_then(|c| c.get("application/json"))
                    .and_then(|c| c.get("schema"))
                    .cloned();

                let route_spec = RouteSpec {
                    path: path.clone(),
                    method: method.to_uppercase(),
                    base_url: "http://localhost:5052".to_string(),
                    response_schema,
                    request_schema,
                };

                self.route_specs.insert(operation_id.to_string(), route_spec);
            }
        }
        Ok(())
    }

    pub fn get_route_spec(&self, operation_id: &str) -> Option<&RouteSpec> {
        self.route_specs.get(operation_id)
    }

    pub fn validate_request(&self, operation_id: &str, request: &Value) -> Result<(), Box<dyn Error>> {
        let route_spec = self.get_route_spec(operation_id)
            .ok_or_else(|| SchemaValidationError { 
                message: format!("No route spec found for {}", operation_id) 
            })?;

        if let Some(schema) = &route_spec.request_schema {
            // TODO: Implement actual JSON Schema validation
            // For now, just check if the request matches the schema structure
            if !self.validate_against_schema(request, schema) {
                return Err(Box::new(SchemaValidationError {
                    message: format!("Request validation failed for {}", operation_id)
                }));
            }
        }
        Ok(())
    }

    pub fn validate_response(&self, operation_id: &str, response: &Value) -> Result<(), Box<dyn Error>> {
        let route_spec = self.get_route_spec(operation_id)
            .ok_or_else(|| SchemaValidationError { 
                message: format!("No route spec found for {}", operation_id) 
            })?;

        if let Some(schema) = &route_spec.response_schema {
            // TODO: Implement actual JSON Schema validation
            // For now, just check if the response matches the schema structure
            if !self.validate_against_schema(response, schema) {
                return Err(Box::new(SchemaValidationError {
                    message: format!("Response validation failed for {}", operation_id)
                }));
            }
        }
        Ok(())
    }

    fn validate_against_schema(&self, value: &Value, schema: &Value) -> bool {
        // Basic schema validation
        if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
            if let Some(obj) = value.as_object() {
                for field in required {
                    if let Some(field_str) = field.as_str() {
                        if !obj.contains_key(field_str) {
                            return false;
                        }
                    }
                }
            }
        }

        if let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) {
            if let Some(obj) = value.as_object() {
                for (key, prop_schema) in properties {
                    if let Some(value) = obj.get(key) {
                        if !self.validate_against_schema(value, prop_schema) {
                            return false;
                        }
                    }
                }
            }
        }

        true
    }
} 