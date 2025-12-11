use ream_light_client::{
    finality_update::LightClientFinalityUpdate, optimistic_update::LightClientOptimisticUpdate,
};
use serde::{Deserialize, Serialize};

/// Light client finality update event.
///
/// The node's latest known LightClientFinalityUpdate has been updated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightClientFinalityUpdateEvent {
    pub version: String,
    pub data: LightClientFinalityUpdate,
}

/// Light client optimistic update event.
///
/// The node's latest known LightClientOptimisticUpdate has been updated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightClientOptimisticUpdateEvent {
    pub version: String,
    pub data: LightClientOptimisticUpdate,
}
