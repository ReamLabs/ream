// use std::collections::HashMap;
// use serde::{Deserialize, Serialize};
// use serde_json::{json, Value};
// use ajv_rs::Ajv;

// use crate::utils::types::{Endpoint, RequestWithBodyCodec, RouteDefinitions, is_request_without_body};
// use crate::utils::wire_format::WireFormat;
// use crate::test::generic_server_test::GenericServerTestCases;
// use crate::test::parse_open_api_spec::{JsonSchema, OpenApiJson, apply_recursively, parse_open_api_spec};

// /// A set of properties that will be ignored during tests execution.
// /// This allows for a black-list mechanism to have a test pass while some part of the spec is not yet implemented.
// ///
// /// Properties can be nested using dot notation, following JSONPath semantic.
// ///
// /// Example:
// /// - query
// /// - query.skip_randao_verification
// #[derive(Debug, Clone, Default)]
// pub struct IgnoredProperty {
//     /// Properties to ignore in the request schema
//     pub request: Option<Vec<String>>,
//     /// Properties to ignore in the response schema
//     pub response: Option<Vec<String>>,
// }

// /// Recursively remove a property from a schema
// ///
// /// @param schema Schema to remove a property from
// /// @param property JSONPath like property to remove from the schema
// fn delete_nested(schema: &mut Option<JsonSchema>, property: &str) {
//     if let Some(schema) = schema {
//         if let Some(properties) = &mut schema.properties {
//             if property.contains('.') {
//                 // Extract first segment, keep the rest as dotted
//                 let parts: Vec<&str> = property.splitn(2, '.').collect();
//                 if let Some(key) = parts.get(0) {
//                     let rest = parts.get(1).unwrap_or(&"");
//                     delete_nested(&mut properties.get_mut(*key).map(|p| p.clone()), rest);
//                 }
//             } else {
//                 // Remove property from 'required'
//                 if let Some(required) = &mut schema.required {
//                     *required = required.iter()
//                         .filter(|e| property != **e)
//                         .cloned()
//                         .collect();
//                 }
//                 // Remove property from 'properties'
//                 properties.remove(property);
//             }
//         }
//     }
// }

// pub fn run_test_check_against_spec<Es>(
//     open_api_json: &OpenApiJson,
//     definitions: &RouteDefinitions<Es>,
//     test_cases: &GenericServerTestCases<Es>,
//     ignored_operations: &[String],
//     ignored_properties: &HashMap<String, IgnoredProperty>
// ) where
//     Es: std::fmt::Debug + Clone,
// {
//     let open_api_spec = parse_open_api_spec(&open_api_json);

//     for (operation_id, route_spec) in open_api_spec.iter() {
//         let is_ignored = ignored_operations.iter().any(|id| id == operation_id);
//         if is_ignored {
//             continue;
//         }

//         let ignored_property = ignored_properties.get(operation_id);

//         // Create Ajv instance
//         let mut ajv = Ajv::new();
//         ajv.add_format("hex", r"^0x[a-fA-F0-9]*$");

//         println!("Testing {}", operation_id);
        
//         let request_schema = &route_spec.request_schema;
//         let response_ok_schema = &route_spec.response_ok_schema;
//         let route_id = operation_id;
        
//         // Get test data and route definition
//         let test_data = test_cases.get(route_id)
//             .expect(&format!("No test data for {}", route_id));
//         let route_def = definitions.get(route_id)
//             .expect(&format!("No route definition for {}", route_id));

//         // Test route method and URL
//         assert_eq!(
//             route_def.method.to_lowercase(), 
//             route_spec.method.to_lowercase(),
//             "{}_route: Method doesn't match", operation_id
//         );
//         assert_eq!(
//             route_def.url, 
//             route_spec.url,
//             "{}_route: URL doesn't match", operation_id
//         );

//         // Test request schema if present
//         if let Some(req_schema) = request_schema {
//             let mut req_json = if is_request_without_body(route_def) {
//                 route_def.req.write_req(&test_data.args)
//             } else {
//                 let req_codec = route_def.req.as_request_with_body_codec()
//                     .expect("Expected RequestWithBodyCodec");
//                 req_codec.write_req_json(&test_data.args)
//             };

//             // Stringify params and query to simulate rendering in HTTP query
//             if let Some(params) = req_json.get_mut("params") {
//                 stringify_properties(params);
//             }
            
//             if let Some(query) = req_json.get_mut("query") {
//                 stringify_properties(query);
//             }

//             // Handle ignored properties for request
//             let mut processed_schema = req_schema.clone();
//             if let Some(ignore_prop) = ignored_property {
//                 if let Some(ignored_properties) = &ignore_prop.request {
//                     for property in ignored_properties {
//                         delete_nested(&mut Some(processed_schema.clone()), property);
//                     }
//                 }
//             }

//             // Validate request
//             validate_schema(&ajv, &processed_schema, &req_json, "request");

//             // Verify that request supports ssz if required by spec
//             if route_spec.request_ssz_required {
//                 let req_codec = route_def.req.as_request_with_body_codec()
//                     .expect("Expected RequestWithBodyCodec");
//                 let req_ssz = req_codec.write_req_ssz(&test_data.args);

//                 assert!(req_ssz.body.is_some(), "Must support ssz request body");
//                 assert_ne!(req_codec.only_support, Some(WireFormat::Json), 
//                           "Must support formats other than JSON");
//             }
//         }

//         // Test response schema if present
//         if let Some(resp_schema) = response_ok_schema {
//             let data = route_def.resp.data.to_json(&test_data.res.data, &test_data.res.meta);
//             let meta_json = route_def.resp.meta.to_json(&test_data.res.meta);
//             let headers = parse_headers(&route_def.resp.meta.to_headers_object(&test_data.res.meta));

//             let res_json = if let Some(transform) = &route_def.resp.transform {
//                 transform.to_response(&data, &meta_json)
//             } else {
//                 let mut combined = data.clone();
//                 combined.as_object_mut().unwrap().extend(meta_json.as_object().unwrap().clone());
//                 combined
//             };

//             // Handle ignored properties for response
//             let mut processed_schema = resp_schema.clone();
//             if let Some(ignore_prop) = ignored_property {
//                 if let Some(ignored_properties) = &ignore_prop.response {
//                     for property in ignored_properties {
//                         delete_nested(&mut Some(processed_schema.clone()), property);
//                     }
//                 }
//             }

//             // Create response object with headers and body
//             let full_response = json!({
//                 "headers": headers,
//                 "body": res_json
//             });

//             // Validate response
//             validate_schema(&ajv, &processed_schema, &full_response, "response");

//             // Verify that response supports ssz if required by spec
//             if route_spec.response_ssz_required {
//                 let ssz_bytes = route_def.resp.data.serialize(&test_data.res.data, &test_data.res.meta);
                
//                 assert!(ssz_bytes.is_some(), "Must support ssz response body");
//                 assert_ne!(route_def.resp.only_support, Some(WireFormat::Json),
//                           "Must support formats other than JSON");
//             }
//         }
//     }
// }

// fn validate_schema(ajv: &Ajv, schema: &JsonSchema, json: &Value, id: &str) {
//     let validation_result = ajv.validate(serde_json::to_value(schema).unwrap(), json.clone());
    
//     if !validation_result.is_valid() {
//         // Create a copy of schema without descriptions for clearer error output
//         let mut schema_without_desc = schema.clone();
//         apply_recursively(&mut schema_without_desc, |obj| {
//             obj.description = None;
//         });

//         let errors = validation_result.errors.unwrap_or_default();
//         let error_strings: Vec<String> = errors.iter()
//             .map(|e| format!("{} - {}", e.instance_path.as_deref().unwrap_or("."), e.message))
//             .collect();

//         panic!(
//             "Invalid {} against spec schema\n\n{}\n\n{}\n\n{}",
//             id,
//             error_strings.join("\n"),
//             serde_json::to_string(&json).unwrap_or_default()[..1000].to_string(),
//             serde_json::to_string(&schema_without_desc).unwrap_or_default()[..1000].to_string()
//         );
//     }
// }

// fn stringify_property(value: &mut Value) {
//     match value {
//         Value::Number(n) => {
//             *value = Value::String(n.to_string());
//         },
//         Value::Array(arr) => {
//             for item in arr.iter_mut() {
//                 stringify_property(item);
//             }
//         },
//         Value::Bool(b) => {
//             *value = Value::String(b.to_string());
//         },
//         Value::Null => {
//             *value = Value::String("null".to_string());
//         },
//         _ => {} // Already a string or object
//     }
// }

// fn stringify_properties(obj: &mut Value) {
//     if let Value::Object(map) = obj {
//         for (_, val) in map.iter_mut() {
//             stringify_property(val);
//         }
//     }
// }

// /// Parse headers before schema validation, the spec expects `{schema: type: boolean}` for
// /// headers with boolean values but values are converted to string when setting the headers
// fn parse_headers(headers: &HashMap<String, String>) -> HashMap<String, Value> {
//     let mut parsed = HashMap::new();
//     for (key, value) in headers {
//         parsed.insert(
//             key.clone(), 
//             if value == "true" || value == "false" {
//                 Value::Bool(value == "true")
//             } else {
//                 Value::String(value.clone())
//             }
//         );
//     }
//     parsed
// }

// // Extensions to the types needed for compatibility with the test framework
// pub trait RequestCodecExt {
//     fn as_request_with_body_codec<E>(&self) -> Option<&RequestWithBodyCodec<E>>;
//     fn write_req(&self, args: &Value) -> Value;
// }

// pub trait ResponseCodecExt {
//     fn to_json(&self, data: &Option<Value>, meta: &Option<Value>) -> Value;
//     fn to_headers_object(&self, meta: &Option<Value>) -> HashMap<String, String>;
//     fn serialize(&self, data: &Option<Value>, meta: &Option<Value>) -> Option<Vec<u8>>;
// }

// pub trait TransformExt {
//     fn to_response(&self, data: &Value, meta: &Value) -> Value;
// }