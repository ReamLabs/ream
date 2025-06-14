use crate::schema::SchemaValidator;
use serde_json::Value;
use std::error::Error;

pub struct TestClient {
    base_url: String,
    validator: SchemaValidator,
}

impl TestClient {
    pub fn new(base_url: String, schema_path: &str) -> Result<Self, Box<dyn Error>> {
        let validator = SchemaValidator::new(schema_path)?;
        Ok(Self {
            base_url,
            validator,
        })
    }

    pub async fn get(&self, operation_id: &str) -> Result<Value, Box<dyn Error>> {
        let route_spec = self.validator.routes.get(operation_id)
            .ok_or_else(|| format!("Operation {} not found in schema", operation_id))?;

        let url = format!("{}{}", self.base_url, route_spec.path);
        println!("Making request to {}", url);

        let response = reqwest::get(&url).await?;
        let response_json = response.json::<Value>().await?;

        println!("Response for {}: {}", operation_id, serde_json::to_string_pretty(&response_json)?);
        self.validator.validate_response(operation_id, &response_json)?;

        Ok(response_json)
    }
} 