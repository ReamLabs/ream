use reqwest::{Client, StatusCode};
use std::error::Error;
use std::fmt;
use serde::{Serialize, Deserialize};
use std::fs;
use std::path::Path;
use std::collections::HashMap;
use regex::Regex;
use errors::ApiError;

#[derive(Debug, Deserialize)]
pub struct Response {
    pub headers: Option<HashMap<String, Header>>,
    pub content: Option<Content>,
}

#[derive(Debug, Deserialize)]
pub struct Header {
    pub schema: JsonSchema,
}

#[derive(Debug, Deserialize)]
pub struct Content {
    #[serde(flatten)]
    #[serde(rename = "contentTypes")]
    pub content_types: HashMap<String, MediaType>,
}

#[derive(Debug, Deserialize)]
pub struct MediaType {
    pub schema: JsonSchema,
}

#[derive(Debug, Deserialize)]
pub struct Parameter {
    pub name: String,
    #[serde(rename = "in")]
    pub param_in: ParameterIn,
    pub schema: JsonSchema,
}

#[derive(Debug, Deserialize)]
pub struct RequestBody {
    pub content: Option<Content>,
}

#[derive(Debug, Default)]
pub struct RespSchema {
    pub headers: Option<JsonSchema>,
    pub body: Option<JsonSchema>,
}

#[derive(Debug, Deserialize)]
pub struct RouteDefinition {
    #[serde(rename = "operationId")]
    pub operation_id: String,
    #[serde(default)]
    pub parameters: Vec<Parameter>,
    pub responses: HashMap<String, Option<Response>>,
    #[serde(rename = "requestBody")]
    pub request_body: Option<RequestBody>,
}

#[derive(Debug, Default)]
pub struct ReqSchema {
    pub params: Option<JsonSchema>,
    pub query: Option<JsonSchema>,
    pub headers: Option<JsonSchema>,
    pub body: Option<JsonSchema>,
}

#[derive(Debug, Clone)]
pub enum ApiError {
    NotFound(String),
    InvalidInput(String),
    Http(StatusCode, String),
}


#[derive(Debug, Clone)]
pub struct OpenApiFile {
    pub url: String,
    pub filepath: String,
    pub version: String,
}

#[derive(Debug, Deserialize)]
pub struct OpenApiJson {
    pub paths: HashMap<String, HashMap<String, serde_json::Value>>,
    pub info: Info,
}

type OperationId = String;
type RouteUrl = String;
type HttpMethod = String;

#[derive(Debug, Deserialize)]
pub struct Info {
    pub version: String,
}

#[derive(Debug, Clone)]
pub struct RouteSpec {
    pub url: RouteUrl,
    pub method: HttpMethod,
    pub response_ok_schema: Option<JsonSchema>,
    pub response_ssz_required: bool,
    pub request_schema: JsonSchema,
    pub request_ssz_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonSchema {
    #[serde(rename = "type")]
    pub schema_type: String,
    pub properties: Option<HashMap<String, JsonSchema>>,
    pub required: Option<Vec<String>>,
    #[serde(rename = "oneOf")]
    pub one_of: Option<Vec<JsonSchema>>,
    #[serde(rename = "allOf")]
    pub all_of: Option<Vec<JsonSchema>>,
    pub nullable: Option<bool>,
    pub description: Option<String>,
    #[serde(rename = "enum")]
    pub enum_values: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ParameterIn {
    Path,
    Query,
    Header,
}

#[derive(Debug)]
enum ContentType {
    Json,
    Ssz,
}

impl ContentType {
    fn as_str(&self) -> &'static str {
        match self {
            ContentType::Json => "application/json",
            ContentType::Ssz => "application/octet-stream",
        }
    }
}

pub fn parse_open_api_spec(open_api_json: &OpenApiJson) -> HashMap<OperationId, RouteSpec> {
    let mut routes = HashMap::new();

    for (route_url, routes_by_method) in &open_api_json.paths {
        for (http_method, route_value) in routes_by_method {
            let route_definition: RouteDefinition = match serde_json::from_value(route_value.clone()) {
                Ok(def) => def,
                Err(e) => {
                    eprintln!("Failed to deserialize route {} {}: {}", http_method, route_url, e);
                    eprintln!("Raw JSON: {}", serde_json::to_string_pretty(route_value).unwrap_or_default());
                    continue;
                },
            };

            let response_ok_schema = build_resp_schema(&route_definition);

            if let Some(schema) = &response_ok_schema {
                match preprocess_schema(&schema) {
                    Ok(processed_schema) => {
                        let response_ssz_required = route_definition
                            .responses
                            .get("200")
                            .and_then(|r| r.as_ref())
                            .and_then(|resp| resp.content.as_ref())
                            .and_then(|content| content.content_types.get(ContentType::Ssz.as_str()))
                            .is_some();

                        let request_schema = build_req_schema(&route_definition);
                        let processed_req_schema = preprocess_schema(&request_schema)
                            .expect("Failed to process request schema");
                        
                        let request_ssz_required = route_definition
                            .request_body
                            .as_ref()
                            .and_then(|rb| rb.content.as_ref())
                            .and_then(|content| content.content_types.get(ContentType::Ssz.as_str()))
                            .is_some();

                        routes.insert(
                            route_definition.operation_id.clone(),
                            RouteSpec {
                                url: route_url.clone(),
                                method: http_method.clone(),
                                response_ok_schema: Some(processed_schema),
                                response_ssz_required,
                                request_schema: processed_req_schema,
                                request_ssz_required,
                            },
                        );
                    }
                    Err(e) => {
                        eprintln!("Error processing response schema: {:?}", e);
                        eprintln!("Schema: {:?}", response_ok_schema);
                    }
                }
            } else {
                let request_schema = build_req_schema(&route_definition);
                let processed_req_schema = preprocess_schema(&request_schema)
                    .expect("Failed to process request schema");
                
                let request_ssz_required = route_definition
                    .request_body
                    .as_ref()
                    .and_then(|rb| rb.content.as_ref())
                    .and_then(|content| content.content_types.get(ContentType::Ssz.as_str()))
                    .is_some();

                let response_ssz_required = route_definition
                    .responses
                    .get("200")
                    .and_then(|r| r.as_ref())
                    .and_then(|resp| resp.content.as_ref())
                    .and_then(|content| content.content_types.get(ContentType::Ssz.as_str()))
                    .is_some();

                routes.insert(
                    route_definition.operation_id.clone(),
                    RouteSpec {
                        url: route_url.clone(),
                        method: http_method.clone(),
                        response_ok_schema: None,
                        response_ssz_required,
                        request_schema: processed_req_schema,
                        request_ssz_required,
                    },
                );
            }
        }
    }

    routes
}

fn preprocess_schema(schema: &JsonSchema) -> Result<JsonSchema, String> {
    let mut processed_schema = schema.clone();
    
    apply_recursively(&mut processed_schema, |obj| {
        if obj.schema_type == "object" && obj.properties.is_some() && obj.required.is_none() {
            if let Some(props) = &obj.properties {
                obj.required = Some(props.keys().cloned().collect());
            }
        }

        obj.nullable = None;

        if let Some(all_of) = &mut obj.all_of {
            if all_of.iter().all(|s| s.enum_values.is_some()) && all_of.len() > 1 {
                let first = all_of.first().cloned();
                if let Some(first_schema) = first {
                    *all_of = vec![first_schema];
                }
            }
        }
    });

    Ok(processed_schema)
}

pub fn apply_recursively<F>(schema: &mut JsonSchema, f: F)
where
    F: Fn(&mut JsonSchema) + Copy,
{
    if let Some(properties) = &mut schema.properties {
        for property in properties.values_mut() {
            apply_recursively(property, f);
        }
    }

    if let Some(one_of) = &mut schema.one_of {
        for item in one_of.iter_mut() {
            apply_recursively(item, f);
        }
    }

    if let Some(all_of) = &mut schema.all_of {
        for item in all_of.iter_mut() {
            apply_recursively(item, f);
        }
    }

    f(schema);
}

fn build_req_schema(route_definition: &RouteDefinition) -> JsonSchema {
    let mut req_schema = ReqSchema::default();

    for parameter in &route_definition.parameters {
        match parameter.param_in {
            ParameterIn::Path => {
                if req_schema.params.is_none() {
                    req_schema.params = Some(JsonSchema {
                        schema_type: "object".to_string(),
                        properties: Some(HashMap::new()),
                        required: None,
                        one_of: None,
                        all_of: None,
                        nullable: None,
                        description: None,
                        enum_values: None,
                    });
                }
                
                if let Some(params) = &mut req_schema.params {
                    if let Some(props) = &mut params.properties {
                        props.insert(parameter.name.clone(), parameter.schema.clone());
                    }
                }
            },
            ParameterIn::Query => {
                if req_schema.query.is_none() {
                    req_schema.query = Some(JsonSchema {
                        schema_type: "object".to_string(),
                        properties: Some(HashMap::new()),
                        required: None,
                        one_of: None,
                        all_of: None,
                        nullable: None,
                        description: None,
                        enum_values: None,
                    });
                }
                
                if let Some(query) = &mut req_schema.query {
                    if let Some(props) = &mut query.properties {
                        props.insert(parameter.name.clone(), parameter.schema.clone());
                    }
                }
            },
            ParameterIn::Header => {
                if req_schema.headers.is_none() {
                    req_schema.headers = Some(JsonSchema {
                        schema_type: "object".to_string(),
                        properties: Some(HashMap::new()),
                        required: None,
                        one_of: None,
                        all_of: None,
                        nullable: None,
                        description: None,
                        enum_values: None,
                    });
                }
                
                if let Some(headers) = &mut req_schema.headers {
                    if let Some(props) = &mut headers.properties {
                        props.insert(parameter.name.clone(), parameter.schema.clone());
                    }
                }
            },
        }
    }

    if let Some(req_body) = &route_definition.request_body {
        if let Some(content) = &req_body.content {
            if let Some(json_schema) = content.content_types.get(ContentType::Json.as_str()) {
                req_schema.body = Some(json_schema.schema.clone());
            }
        }
    }

    let mut properties = HashMap::new();
    
    if let Some(params) = req_schema.params {
        properties.insert("params".to_string(), params);
    }
    
    if let Some(query) = req_schema.query {
        properties.insert("query".to_string(), query);
    }
    
    if let Some(headers) = req_schema.headers {
        properties.insert("headers".to_string(), headers);
    }
    
    if let Some(body) = req_schema.body {
        properties.insert("body".to_string(), body);
    }

    JsonSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: None,
        one_of: None,
        all_of: None,
        nullable: None,
        description: None,
        enum_values: None,
    }
}

fn build_resp_schema(route_definition: &RouteDefinition) -> Option<JsonSchema> {
    let mut resp_schema = RespSchema::default();

    if let Some(Some(response_ok)) = route_definition.responses.get("200") {
        if let Some(headers) = &response_ok.headers {
            let mut header_properties = HashMap::new();
            
            for (header_name, header) in headers {
                header_properties.insert(header_name.clone(), header.schema.clone());
            }
            
            if !header_properties.is_empty() {
                resp_schema.headers = Some(JsonSchema {
                    schema_type: "object".to_string(),
                    properties: Some(header_properties),
                    required: None,
                    one_of: None,
                    all_of: None,
                    nullable: None,
                    description: None,
                    enum_values: None,
                });
            }
        }

        if let Some(content) = &response_ok.content {
            if let Some(json_schema) = content.content_types.get(ContentType::Json.as_str()) {
                resp_schema.body = Some(json_schema.schema.clone());
            }
        }
    }

    if resp_schema.headers.is_none() && resp_schema.body.is_none() {
        return None;
    }

    let mut properties = HashMap::new();
    
    if let Some(headers) = resp_schema.headers {
        properties.insert("headers".to_string(), headers);
    }
    
    if let Some(body) = resp_schema.body {
        properties.insert("body".to_string(), body);
    }

    Some(JsonSchema {
        schema_type: "object".to_string(),
        properties: Some(properties),
        required: None,
        one_of: None,
        all_of: None,
        nullable: None,
        description: None,
        enum_values: None,
    })
}