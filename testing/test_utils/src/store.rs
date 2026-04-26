use ream_consensus_lean::{
    block::{BlockSignatures, SignedBlock},
    utils::generate_default_validators,
};
use ream_fork_choice_lean::{genesis::setup_genesis, store::Store};
use ream_network_spec::networks::{LeanNetworkSpec, lean_network_spec, set_lean_network_spec};
use ream_post_quantum_crypto::leansig::signature::Signature;
use ream_storage::db::ReamDB;
use ssz_types::VariableList;

pub async fn sample_store(no_of_validators: usize) -> Store {
    set_lean_network_spec(LeanNetworkSpec::ephemery().into());
    let (genesis_block, genesis_state) = setup_genesis(
        lean_network_spec().genesis_time,
        generate_default_validators(no_of_validators),
    );

    let signed_genesis_block = SignedBlock {
        block: genesis_block,
        signature: BlockSignatures {
            attestation_signatures: VariableList::default(),
            proposer_signature: Signature::blank(),
        },
    };

    let temp_path = std::env::temp_dir().join(format!(
        "lean_test_{}_{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&temp_path).expect("Failed to create temp directory");
    let ream_db = ReamDB::new(temp_path).expect("Failed to init Ream Database");
    let lean_db = ream_db.init_lean_db().expect("Failed to init lean db");

    Store::get_forkchoice_store(signed_genesis_block, genesis_state, lean_db, Some(0), None)
        .expect("Failed to create forkchoice store")
}
