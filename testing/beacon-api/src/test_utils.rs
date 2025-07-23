use crate::schema::SchemaValidator;
use serde_json::{ Value};
use std::collections::HashMap;
use std::error::Error;
use std::fs;

pub struct TestClient {
    validator: SchemaValidator,
}

impl TestClient {
    pub fn new(schema_path: &str) -> Result<Self, Box<dyn Error>> {
        let schema = fs::read_to_string(schema_path)?;
        let schema: Value = serde_json::from_str(&schema)?;
        let validator = SchemaValidator::new(schema)?;
        Ok(Self { validator })
    }

    pub fn validate_request(&self, operation_id: &str, request: &Value) -> Result<(), Box<dyn Error>> {
        self.validator.validate_request(operation_id, request)
    }

    pub fn validate_response(&self, operation_id: &str, response: &Value) -> Result<(), Box<dyn Error>> {
        self.validator.validate_response(operation_id, response)
    }

    pub fn get_route_spec(&self, operation_id: &str) -> Option<&crate::schema::RouteSpec> {
        self.validator.get_route_spec(operation_id)
    }

    // Helper function to convert camelCase to snake_case
    fn to_snake_case(s: &str) -> String {
        let mut result = String::new();
        for (i, c) in s.chars().enumerate() {
            if c.is_uppercase() {
                if i > 0 {
                    result.push('_');
                }
                result.push(c.to_lowercase().next().unwrap());
            } else {
                result.push(c);
            }
        }
        result
    }

    // Helper function to replace path parameters
    fn replace_path_params(&self, path: &str, args: &Value) -> String {
        let mut replaced_path = path.to_string();
        if let Some(map) = args.as_object() {
            for (key, value) in map {
                let snake_key = Self::to_snake_case(key);
                let placeholder = format!("{{{}}}", snake_key);
                let value_str = match value {
                    Value::String(s) => s.clone(),
                    Value::Array(arr) => {
                        arr.iter()
                            .map(|v| v.to_string())
                            .collect::<Vec<_>>()
                            .join(",")
                    },
                    _ => value.to_string()
                };
                replaced_path = replaced_path.replace(&placeholder, &value_str);
            }
        }
        replaced_path
    }

    // Helper function to extract query parameters (only the unused args)
    fn extract_query_params(&self, path: &str, args: &Value) -> Vec<(String, String)> {
        let mut query_params = Vec::new();
        if let Some(map) = args.as_object() {
            for (key, value) in map {
                let snake_key = Self::to_snake_case(key);
                let placeholder = format!("{{{}}}", snake_key);
                if !path.contains(&placeholder) {
                    let value_str = match value {
                        Value::String(s) => s.clone(),
                        Value::Array(arr) => {
                            arr.iter()
                                .map(|v| v.to_string())
                                .collect::<Vec<_>>()
                                .join(",")
                        },
                        _ => value.to_string()
                    };
                    query_params.push((key.clone(), value_str));
                }
            }
        }
        query_params
    }

    pub async fn get(&self, operation_id: &str) -> Result<Value, Box<dyn Error>> {
        let route_spec = self.get_route_spec(operation_id)
            .ok_or_else(|| format!("Operation {} not found in schema", operation_id))?;

        let url = format!("{}{}", route_spec.base_url, route_spec.path);
        println!("Making request to {}", url);

        let response = reqwest::get(&url).await?;
        let response_json = response.json::<Value>().await?;

        println!("Response in json : {}", response_json);
        println!("Response for {}: {}", operation_id, serde_json::to_string_pretty(&response_json)?);
        self.validate_response(operation_id, &response_json)?;

        Ok(response_json)
    }

    pub async fn get_with_args(&self, operation_id: &str, args: Value) -> Result<Value, Box<dyn Error>> {
        let route_spec = self.get_route_spec(operation_id)
            .ok_or_else(|| format!("Operation {} not found in schema", operation_id))?;

        // Replace path params
        let path = self.replace_path_params(&route_spec.path, &args);

        // Handle query params
        let query_params = self.extract_query_params(&route_spec.path, &args);
        let mut url = format!("{}{}", route_spec.base_url, path);
        if !query_params.is_empty() {
            let query_string = query_params
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("&");
            url.push('?');
            url.push_str(&query_string);
        }

        println!("Making request to {}", url);

        let response = reqwest::get(&url).await?;
        let response_json = response.json::<Value>().await?;

        println!("Response in json : {}", response_json);
        println!("Response for {}: {}", operation_id, serde_json::to_string_pretty(&response_json)?);
        self.validate_response(operation_id, &response_json)?;

        Ok(response_json)
    }

    pub async fn post_with_args(&self, operation_id: &str, args: Value) -> Result<Value, Box<dyn Error>> {
        let route_spec = self.get_route_spec(operation_id)
            .ok_or_else(|| format!("Operation {} not found in schema", operation_id))?;

        // Replace path params
        let path = self.replace_path_params(&route_spec.path, &args);
        let url = format!("{}{}", route_spec.base_url, path);
        println!("Making POST request to {}", url);

        let client = reqwest::Client::new();
        let response = client.post(&url)
            .json(&args)
            .send()
            .await?;

        let response_json = response.json::<Value>().await?;
        println!("Response in json : {}", response_json);
        println!("Response for {}: {}", operation_id, serde_json::to_string_pretty(&response_json)?);
        self.validate_response(operation_id, &response_json)?;

        Ok(response_json)
    }
}
