// use std::collections::HashMap;
// use serde::{Deserialize, Serialize};
// use regex::Regex;

#[derive(Debug, Clone)]
pub struct OpenApiFile {
    pub url: String,
    pub filepath: String,
    pub version: Regex,
}

// /// "getBlockRoot"
// type OperationId = String;
// /// "/eth/v1/beacon/blocks/{block_id}/root"
// type RouteUrl = String;
// /// "get" | "post"
// type HttpMethod = String;

// #[skip_serializing_none]
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct JsonSchema {
//     #[serde(rename = "type")]
//     pub schema_type: String,
//     pub properties: Option<HashMap<String, JsonSchema>>,
//     pub required: Option<Vec<String>>,
//     pub one_of: Option<Vec<JsonSchema>>,
//     pub all_of: Option<Vec<JsonSchema>>,
//     pub nullable: Option<bool>,
//     pub description: Option<String>,
//     pub enum_values: Option<Vec<String>>,
// }

#[derive(Debug, Deserialize)]
pub struct OpenApiJson {
    pub paths: HashMap<RouteUrl, HashMap<HttpMethod, RouteDefinition>>,
    pub info: Info,
}

#[derive(Debug, Deserialize)]
pub struct Info {
    pub version: String,
}

// #[derive(Debug, Deserialize)]
// pub struct Content {
//     #[serde(flatten)]
//     pub content_types: HashMap<String, ContentTypeSchema>,
// }

// #[derive(Debug, Deserialize)]
// #[skip_serializing_none]
// pub struct ContentTypeSchema {
//     pub schema: JsonSchema,
//     pub examples: Option<HashMap<String, Example>>,
// }

// #[derive(Debug, Deserialize)]
// pub struct Example {
//     pub description: String,
//     pub value: serde_json::Value,
// }

// #[derive(Debug, Deserialize, PartialEq)]
// #[serde(rename_all = "lowercase")]
// pub enum ParameterIn {
//     Path,
//     Query,
//     Header,
// }

// #[derive(Debug, Deserialize)]
// pub struct Parameter {
//     pub name: String,
//     #[serde(rename = "in")]
//     pub param_in: ParameterIn,
//     pub schema: JsonSchema,
// }

// #[derive(Debug, Deserialize)]
// pub struct RouteDefinition {
//     pub operation_id: String,
//     #[serde(default)]
//     pub parameters: Vec<Parameter>,
//     pub responses: HashMap<String, Option<Response>>,
//     pub request_body: Option<RequestBody>,
// }

// #[derive(Debug, Deserialize)]
// pub struct Response {
//     pub headers: Option<HashMap<String, Header>>,
//     pub content: Option<Content>,
// }

// #[derive(Debug, Deserialize)]
// pub struct Header {
//     pub schema: JsonSchema,
// }

// #[derive(Debug, Deserialize)]
// pub struct RequestBody {
//     pub content: Option<Content>,
// }

// /// Route specification extracted from OpenAPI
// #[derive(Debug, Clone)]
// pub struct RouteSpec {
//     pub url: RouteUrl,
//     pub method: HttpMethod,
//     pub response_ok_schema: Option<JsonSchema>,
//     pub response_ssz_required: bool,
//     pub request_schema: JsonSchema,
//     pub request_ssz_required: bool,
// }

// /// Request schema components
// #[derive(Debug, Default)]
// pub struct ReqSchema {
//     pub params: Option<JsonSchema>,
//     pub query: Option<JsonSchema>,
//     pub headers: Option<JsonSchema>,
//     pub body: Option<JsonSchema>,
// }

// /// Response schema components
// #[derive(Debug, Default)]
// pub struct RespSchema {
//     pub headers: Option<JsonSchema>,
//     pub body: Option<JsonSchema>,
// }

// /// Status codes
// #[derive(Debug)]
// enum StatusCode {
//     OK,
// }

// impl StatusCode {
//     fn as_str(&self) -> &'static str {
//         match self {
//             StatusCode::OK => "200",
//         }
//     }
// }

// /// Content types
// #[derive(Debug)]
// enum ContentType {
//     Json,
//     Ssz,
// }

// impl ContentType {
//     fn as_str(&self) -> &'static str {
//         match self {
//             ContentType::Json => "application/json",
//             ContentType::Ssz => "application/octet-stream",
//         }
//     }
// }

// pub fn parse_open_api_spec(open_api_json: &OpenApiJson) -> HashMap<OperationId, RouteSpec> {
//     let mut routes = HashMap::new();

//     for (route_url, routes_by_method) in &open_api_json.paths {
//         for (http_method, route_definition) in routes_by_method {
//             let response_ok_schema = build_resp_schema(route_definition);

//             // Force all properties to have required, else validation won't check missing properties
//             if let Some(schema) = &response_ok_schema {
//                 match preprocess_schema(&schema) {
//                     Ok(processed_schema) => {
//                         let response_ssz_required = route_definition
//                             .responses
//                             .get(StatusCode::OK.as_str())
//                             .and_then(|r| r.as_ref())
//                             .and_then(|resp| resp.content.as_ref())
//                             .and_then(|content| content.content_types.get(ContentType::Ssz.as_str()))
//                             .is_some();

//                         let request_schema = build_req_schema(route_definition);
//                         let processed_req_schema = preprocess_schema(&request_schema)
//                             .expect("Failed to process request schema");
                        
//                         let request_ssz_required = route_definition
//                             .request_body
//                             .as_ref()
//                             .and_then(|rb| rb.content.as_ref())
//                             .and_then(|content| content.content_types.get(ContentType::Ssz.as_str()))
//                             .is_some();

//                         routes.insert(
//                             route_definition.operation_id.clone(),
//                             RouteSpec {
//                                 url: route_url.clone(),
//                                 method: http_method.clone(),
//                                 response_ok_schema: Some(processed_schema),
//                                 response_ssz_required,
//                                 request_schema: processed_req_schema,
//                                 request_ssz_required,
//                             },
//                         );
//                     }
//                     Err(e) => {
//                         eprintln!("Error processing response schema: {:?}", e);
//                         eprintln!("Schema: {:?}", response_ok_schema);
//                     }
//                 }
//             } else {
//                 let request_schema = build_req_schema(route_definition);
//                 let processed_req_schema = preprocess_schema(&request_schema)
//                     .expect("Failed to process request schema");
                
//                 let request_ssz_required = route_definition
//                     .request_body
//                     .as_ref()
//                     .and_then(|rb| rb.content.as_ref())
//                     .and_then(|content| content.content_types.get(ContentType::Ssz.as_str()))
//                     .is_some();

//                 let response_ssz_required = route_definition
//                     .responses
//                     .get(StatusCode::OK.as_str())
//                     .and_then(|r| r.as_ref())
//                     .and_then(|resp| resp.content.as_ref())
//                     .and_then(|content| content.content_types.get(ContentType::Ssz.as_str()))
//                     .is_some();

//                 routes.insert(
//                     route_definition.operation_id.clone(),
//                     RouteSpec {
//                         url: route_url.clone(),
//                         method: http_method.clone(),
//                         response_ok_schema: None,
//                         response_ssz_required,
//                         request_schema: processed_req_schema,
//                         request_ssz_required,
//                     },
//                 );
//             }
//         }
//     }

//     routes
// }

// /// Process a JSON schema to enforce certain properties
// fn preprocess_schema(schema: &JsonSchema) -> Result<JsonSchema, String> {
//     let mut processed_schema = schema.clone();
    
//     // Apply transformations recursively
//     apply_recursively(&mut processed_schema, |obj| {
//         // Require all properties
//         if obj.schema_type == "object" && obj.properties.is_some() && obj.required.is_none() {
//             if let Some(props) = &obj.properties {
//                 obj.required = Some(props.keys().cloned().collect());
//             }
//         }

//         // Remove nullable
//         obj.nullable = None;

//         // Remove non-intersecting allOf enum
//         if let Some(all_of) = &mut obj.all_of {
//             if all_of.iter().all(|s| s.enum_values.is_some()) && all_of.len() > 1 {
//                 let first = all_of.first().cloned();
//                 if let Some(first_schema) = first {
//                     *all_of = vec![first_schema];
//                 }
//             }
//         }
//     });

//     Ok(processed_schema)
// }

// /// Apply a function recursively to all parts of a JSON schema
// pub fn apply_recursively<F>(schema: &mut JsonSchema, f: F)
// where
//     F: Fn(&mut JsonSchema),
// {
//     // Handle properties
//     if let Some(properties) = &mut schema.properties {
//         for property in properties.values_mut() {
//             apply_recursively(property, &f);
//         }
//     }

//     // Handle oneOf
//     if let Some(one_of) = &mut schema.one_of {
//         for item in one_of.iter_mut() {
//             apply_recursively(item, &f);
//         }
//     }

//     // Handle allOf
//     if let Some(all_of) = &mut schema.all_of {
//         for item in all_of.iter_mut() {
//             apply_recursively(item, &f);
//         }
//     }

//     // Apply the function to this schema node
//     f(schema);
// }

// /// Build a request schema from a route definition
// fn build_req_schema(route_definition: &RouteDefinition) -> JsonSchema {
//     let mut req_schema = ReqSchema::default();

//     // Process parameters
//     for parameter in &route_definition.parameters {
//         match parameter.parameter_in {
//             ParameterIn::Path => {
//                 if req_schema.params.is_none() {
//                     req_schema.params = Some(JsonSchema {
//                         schema_type: "object".to_string(),
//                         properties: Some(HashMap::new()),
//                         required: None,
//                         one_of: None,
//                         all_of: None,
//                         nullable: None,
//                         description: None,
//                         enum_values: None,
//                     });
//                 }
                
//                 if let Some(params) = &mut req_schema.params {
//                     if let Some(props) = &mut params.properties {
//                         props.insert(parameter.name.clone(), parameter.schema.clone());
//                     }
//                 }
//             },
//             ParameterIn::Query => {
//                 if req_schema.query.is_none() {
//                     req_schema.query = Some(JsonSchema {
//                         schema_type: "object".to_string(),
//                         properties: Some(HashMap::new()),
//                         required: None,
//                         one_of: None,
//                         all_of: None,
//                         nullable: None,
//                         description: None,
//                         enum_values: None,
//                     });
//                 }
                
//                 if let Some(query) = &mut req_schema.query {
//                     if let Some(props) = &mut query.properties {
//                         props.insert(parameter.name.clone(), parameter.schema.clone());
//                     }
//                 }
//             },
//             ParameterIn::Header => {
//                 if req_schema.headers.is_none() {
//                     req_schema.headers = Some(JsonSchema {
//                         schema_type: "object".to_string(),
//                         properties: Some(HashMap::new()),
//                         required: None,
//                         one_of: None,
//                         all_of: None,
//                         nullable: None,
//                         description: None,
//                         enum_values: None,
//                     });
//                 }
                
//                 if let Some(headers) = &mut req_schema.headers {
//                     if let Some(props) = &mut headers.properties {
//                         props.insert(parameter.name.clone(), parameter.schema.clone());
//                     }
//                 }
//             },
//         }
//     }

//     // Process request body
//     if let Some(req_body) = &route_definition.request_body {
//         if let Some(content) = &req_body.content {
//             if let Some(json_schema) = content.content_types.get(ContentType::Json.as_str()) {
//                 req_schema.body = Some(json_schema.schema.clone());
//             }
//         }
//     }

//     // Create final schema
//     let mut properties = HashMap::new();
    
//     if let Some(params) = req_schema.params {
//         properties.insert("params".to_string(), params);
//     }
    
//     if let Some(query) = req_schema.query {
//         properties.insert("query".to_string(), query);
//     }
    
//     if let Some(headers) = req_schema.headers {
//         properties.insert("headers".to_string(), headers);
//     }
    
//     if let Some(body) = req_schema.body {
//         properties.insert("body".to_string(), body);
//     }

//     JsonSchema {
//         schema_type: "object".to_string(),
//         properties: Some(properties),
//         required: None,
//         one_of: None,
//         all_of: None,
//         nullable: None,
//         description: None,
//         enum_values: None,
//     }
// }

// /// Build a response schema from a route definition
// fn build_resp_schema(route_definition: &RouteDefinition) -> Option<JsonSchema> {
//     let mut resp_schema = RespSchema::default();

//     // Process response headers
//     if let Some(Some(response_ok)) = route_definition.responses.get(StatusCode::OK.as_str()) {
//         if let Some(headers) = &response_ok.headers {
//             let mut header_properties = HashMap::new();
            
//             for (header_name, header) in headers {
//                 header_properties.insert(header_name.clone(), header.schema.clone());
//             }
            
//             if !header_properties.is_empty() {
//                 resp_schema.headers = Some(JsonSchema {
//                     schema_type: "object".to_string(),
//                     properties: Some(header_properties),
//                     required: None,
//                     one_of: None,
//                     all_of: None,
//                     nullable: None,
//                     description: None,
//                     enum_values: None,
//                 });
//             }
//         }

//         // Process response body
//         if let Some(content) = &response_ok.content {
//             if let Some(json_schema) = content.content_types.get(ContentType::Json.as_str()) {
//                 resp_schema.body = Some(json_schema.schema.clone());
//             }
//         }
//     }

//     // If no response components, return None
//     if resp_schema.headers.is_none() && resp_schema.body.is_none() {
//         return None;
//     }

//     // Create final schema
//     let mut properties = HashMap::new();
    
//     if let Some(headers) = resp_schema.headers {
//         properties.insert("headers".to_string(), headers);
//     }
    
//     if let Some(body) = resp_schema.body {
//         properties.insert("body".to_string(), body);
//     }

//     Some(JsonSchema {
//         schema_type: "object".to_string(),
//         properties: Some(properties),
//         required: None,
//         one_of: None,
//         all_of: None,
//         nullable: None,
//         description: None,
//         enum_values: None,
//     })
// }

