pub mod genesis;
pub mod version;

use serde::Serialize;
use serde_json::json;
use warp::reply::{Json, json};

/// A generic data struct that can be used to wrap any data type
/// used for json rpc responses
///
/// # Example
/// {
///  "data": json!(T)
/// }
#[derive(Debug, Serialize)]
pub struct Data<T> {
    pub data: T,
}

impl<T: Serialize> Data<T> {
    pub fn json(data: T) -> Json {
        json(&json!(Self { data }))
    }
}
