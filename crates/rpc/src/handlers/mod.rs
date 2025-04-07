pub mod block;
pub mod checkpoint;
pub mod config;
pub mod fork;
pub mod genesis;
pub mod randao;
pub mod state;
pub mod validator;
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

/// A BeaconResponse data struct that can be used to wrap data type
/// used for json rpc responses
///
/// # Example
/// {
///  "data": json!({
///     "execution_optimistic" : bool,
///     "finalized" : bool,
///     "data" : T
/// })
/// }
#[derive(Debug, Serialize)]
pub struct BeaconResponse<T> {
    pub execution_optimistic: bool,
    pub finalized: bool,
    pub data: T,
}

impl<T: Serialize> BeaconResponse<T> {
    pub fn json(data: T) -> Json {
        json(&json!(Self {
            data,
            execution_optimistic: EXECUTION_OPTIMISTIC,
            finalized: FINALIZED
        }))
    }
}

#[derive(Debug, Serialize)]
pub struct BeaconVersionedResponse<T> {
    pub version: String,
    pub execution_optimistic: bool,
    pub finalized: bool,
    pub data: T,
}

impl<T: Serialize> BeaconVersionedResponse<T> {
    pub fn json(data: T) -> Json {
        json(&json!(Self {
            version: String::from("electra"),
            data,
            execution_optimistic: EXECUTION_OPTIMISTIC,
            finalized: FINALIZED
        }))
    }
}
