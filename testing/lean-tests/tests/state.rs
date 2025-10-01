use ream_consensus_lean::{config::Config, state::LeanState};
use ream_consensus_misc::constants::lean::VALIDATOR_REGISTRY_LIMIT;

#[test]
fn test_placeholder() {
    let sample_config = Config {
        num_validators: VALIDATOR_REGISTRY_LIMIT,
        genesis_time: 0,
    };

    let genesis_state = LeanState::new(VALIDATOR_REGISTRY_LIMIT, 0);

    
}
