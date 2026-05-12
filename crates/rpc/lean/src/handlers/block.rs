use actix_web::{
    HttpRequest, HttpResponse, Responder, get,
    http::header,
    web::{Data, Path},
};
use ream_api_types_common::{
    content_type::{ContentType, JSON_CONTENT_TYPE, SSZ_CONTENT_TYPE},
    error::ApiError,
    id::ID,
};
use ream_consensus_lean::block::{Block, SignedBlock};
use ream_fork_choice_lean::store::LeanStoreReader;
use ream_storage::tables::{field::REDBField, table::REDBTable};
use ssz::Encode;

// GET /lean/v0/blocks/{block_id}
#[get("/blocks/{block_id}")]
pub async fn get_block(
    block_id: Path<ID>,
    lean_chain: Data<LeanStoreReader>,
) -> Result<impl Responder, ApiError> {
    Ok(HttpResponse::Ok().json(
        get_block_by_id(block_id.into_inner(), lean_chain)
            .await?
            .ok_or_else(|| ApiError::NotFound("Block not found".to_string()))?,
    ))
}

// GET /lean/v0/blocks/finalized
#[get("/blocks/finalized")]
pub async fn get_finalized_signed_block(
    http_request: HttpRequest,
    lean_chain: Data<LeanStoreReader>,
) -> Result<impl Responder, ApiError> {
    let signed_block = get_finalized_signed_block_inner(lean_chain).await?;

    match ContentType::from(http_request.headers().get(header::ACCEPT)) {
        ContentType::Ssz => Ok(HttpResponse::Ok()
            .content_type(SSZ_CONTENT_TYPE)
            .body(signed_block.as_ssz_bytes())),
        ContentType::Json => Ok(HttpResponse::Ok()
            .content_type(JSON_CONTENT_TYPE)
            .json(signed_block.block)),
    }
}

async fn get_finalized_signed_block_inner(
    lean_chain: Data<LeanStoreReader>,
) -> Result<SignedBlock, ApiError> {
    let lean_chain = lean_chain.read().await;
    let store = lean_chain.store.lock().await;

    let finalized_root = store
        .latest_finalized_provider()
        .get()
        .map_err(|err| ApiError::InternalError(format!("No latest finalized hash: {err:?}")))?
        .root;

    store
        .block_provider()
        .get(finalized_root)
        .map_err(|err| ApiError::InternalError(format!("DB error: {err}")))?
        .ok_or_else(|| ApiError::NotFound("Finalized signed block not available".to_string()))
}

// Retrieve a block from the lean chain by its block ID.
pub async fn get_block_by_id(
    block_id: ID,
    lean_chain: Data<LeanStoreReader>,
) -> Result<Option<Block>, ApiError> {
    let lean_chain = lean_chain.read().await;
    let block_root = match block_id {
        ID::Finalized => lean_chain
            .store
            .lock()
            .await
            .latest_finalized_provider()
            .get()
            .map(|checkpoint| checkpoint.root)
            .map_err(|err| ApiError::InternalError(format!("No latest finalized hash: {err:?}"))),
        ID::Genesis => {
            return Err(ApiError::NotFound(
                "This ID type is currently not supported".to_string(),
            ));
        }
        ID::Head => lean_chain
            .store
            .lock()
            .await
            .head_provider()
            .get()
            .map_err(|err| ApiError::InternalError(format!("Could not get head: {err:?}"))),
        ID::Justified => lean_chain
            .store
            .lock()
            .await
            .latest_justified_provider()
            .get()
            .map(|checkpoint| checkpoint.root)
            .map_err(|err| ApiError::InternalError(format!("No latest justified hash: {err:?}"))),
        ID::Slot(slot) => lean_chain
            .get_block_id_by_slot(slot)
            .await
            .map_err(|err| ApiError::InternalError(format!("No block for slot {slot}: {err:?}"))),
        ID::Root(root) => Ok(root),
    };

    let provider = lean_chain.store.clone().lock().await.block_provider();
    provider
        .get(block_root?)
        .map(|maybe_signed_block| maybe_signed_block.map(|signed_block| signed_block.block.clone()))
        .map_err(|err| ApiError::InternalError(format!("DB error: {err}")))
}

#[cfg(test)]
mod tests {
    use actix_web::{App, http::StatusCode, test, web::Data};
    use ream_consensus_lean::{block::SignedBlock, state::LeanState};
    use ream_sync::rwlock::Writer;
    use ream_test_utils::store::sample_store;
    use ssz::Decode;
    use tree_hash::TreeHash;

    use super::get_finalized_signed_block;

    #[tokio::test]
    async fn test_get_finalized_signed_block_returns_ssz() {
        let store = sample_store(10).await;
        let (_writer, reader) = Writer::new(store);

        let app = test::init_service(
            App::new()
                .app_data(Data::new(reader))
                .service(get_finalized_signed_block),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/blocks/finalized")
            .insert_header(("Accept", "application/octet-stream"))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "application/octet-stream"
        );

        let body = test::read_body(resp).await;
        SignedBlock::from_ssz_bytes(&body).expect("Failed to decode SSZ SignedBlock");
    }

    #[tokio::test]
    async fn test_finalized_signed_block_state_root_matches_finalized_state() {
        // Protocol invariant required by `Store::get_forkchoice_store`:
        // `anchor_block.state_root == hash_tree_root(state)`.
        use crate::handlers::state::get_state;

        let store = sample_store(10).await;
        let (_writer, reader) = Writer::new(store);

        let app = test::init_service(
            App::new()
                .app_data(Data::new(reader))
                .service(get_finalized_signed_block)
                .service(get_state),
        )
        .await;

        let block_req = test::TestRequest::get()
            .uri("/blocks/finalized")
            .insert_header(("Accept", "application/octet-stream"))
            .to_request();
        let block_resp = test::call_service(&app, block_req).await;
        let block_body = test::read_body(block_resp).await;
        let signed_block =
            SignedBlock::from_ssz_bytes(&block_body).expect("Failed to decode SignedBlock");

        let state_req = test::TestRequest::get()
            .uri("/states/finalized")
            .insert_header(("Accept", "application/octet-stream"))
            .to_request();
        let state_resp = test::call_service(&app, state_req).await;
        let state_body = test::read_body(state_resp).await;
        let state = LeanState::from_ssz_bytes(&state_body).expect("Failed to decode LeanState");

        assert_eq!(signed_block.block.state_root, state.tree_hash_root());
    }
}
