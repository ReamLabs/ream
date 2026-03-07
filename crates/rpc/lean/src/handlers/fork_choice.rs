use actix_web::{HttpResponse, Responder, get, web::Data};
use ream_api_types_common::error::ApiError;
use ream_fork_choice_lean::store::LeanStoreReader;
use ream_storage::tables::{field::REDBField, table::REDBTable};
use tree_hash::TreeHash;

#[get("/fork_choice")]
pub async fn get_fork_choice_tree(
    lean_chain: Data<LeanStoreReader>,
) -> Result<impl Responder, ApiError> {
    let lean_chain = lean_chain.read().await;
    let weight_map = lean_chain.compute_block_weights().await;
    let db = lean_chain.store.lock().await;

    let finalized_checkpoint = db.latest_finalized_provider().get().map_err(|err| {
        ApiError::InternalError(format!(
            "Unable to get latest finalized checkpoint: {err:?}"
        ))
    })?;

    let finalized_root = finalized_checkpoint.root;
    let finalized_slot = finalized_checkpoint.slot;

    let safe_target = db
        .safe_target_provider()
        .get()
        .map_err(|err| ApiError::InternalError(format!("Unable to get safe target: {err:?}")))?;

    let justified_checkpoint = db.latest_justified_provider().get().map_err(|err| {
        ApiError::InternalError(format!(
            "Unable to get latest justified checkpoint: {err:?}"
        ))
    })?;

    let justified_root = justified_checkpoint.root;
    let justified_slot = justified_checkpoint.slot;

    let head_root = db.head_provider().get().map_err(|err| {
        ApiError::InternalError(format!("Unable to get latest head root: {err:?}"))
    })?;

    let head_state = db
        .state_provider()
        .get(head_root)
        .map_err(|err| ApiError::InternalError(format!("Unable to get head state: {err:?}")))?;

    let validator_count = head_state
        .map(|state| state.validators.len() as u64)
        .unwrap_or(0);

    let blocks = db
        .block_provider()
        .get_all_blocks(finalized_slot)
        .unwrap_or(vec![])
        .iter()
        .map(|block| {
            let root = block.message.block.tree_hash_root();
            let weight = weight_map
                .as_ref()
                .map(|weight_map| weight_map.get(&root).cloned().unwrap_or(0))
                .unwrap_or(0);
            serde_json::json!({
                "root": root,
                "slot": block.message.block.slot,
                "parent_root": block.message.block.parent_root,
                "proposer_index": block.message.block.proposer_index,
                "weight": weight,
            })
        })
        .collect::<Vec<_>>();

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "nodes": blocks,
        "head": head_root,
        "justified": {
            "slot": justified_slot,
            "root": justified_root,
        },
        "finalized": {
            "slot": finalized_slot,
            "root": finalized_root,
        },
        "safe_target": safe_target,
        "validator_count": validator_count,
    })))
}

#[cfg(test)]
mod tests {
    use actix_web::{App, http::StatusCode, test, web::Data};
    use ream_sync::rwlock::Writer;
    use ream_test_utils::store::sample_store;

    use super::*;

    #[tokio::test]
    async fn test_get_fork_choice_tree_genesis() {
        let store = sample_store(5).await;
        let (_writer, reader) = Writer::new(store);

        let app = test::init_service(
            App::new()
                .app_data(Data::new(reader))
                .service(get_fork_choice_tree),
        )
        .await;

        let req = test::TestRequest::get().uri("/fork_choice").to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body = test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&body).expect("Failed to decode JSON");

        let validator_count = json
            .get("validator_count")
            .expect("validator_count field missing");
        assert_eq!(
            validator_count
                .as_u64()
                .expect("validator_count must be a number"),
            5
        );

        let nodes = json
            .get("nodes")
            .expect("nodes field missing")
            .as_array()
            .expect("nodes must be an array");

        assert_eq!(nodes.len(), 1);

        let genesis_block = nodes.first().expect("At least one block should exist");
        assert_eq!(genesis_block.get("slot").unwrap().as_u64().unwrap(), 0);
    }

    #[tokio::test]
    async fn test_get_fork_choice_tree_uninitialized() {
        let app = test::init_service(App::new().service(get_fork_choice_tree)).await;

        let req = test::TestRequest::get().uri("/fork_choice").to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
