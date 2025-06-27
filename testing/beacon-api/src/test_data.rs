use serde_json::{json, Value};
use std::collections::HashMap;

const ROOT: &str = "0x4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95";
const RANDAO: &str = "0x0101010101010101010101010101010101010101010101010101010101010101";
const BALANCE: u64 = 32_000_000_000;
const REWARD: u64 = 32_000_000_000;
const PUBKEY_HEX: &str = "0x010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101";
const SIGNATURE: &str = "0x010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101";

fn create_block_header_response() -> Value {
    json!({
        "root": ROOT,
        "canonical": true,
        "header": {
            "message": {
                "slot": "1",
                "proposer_index": "1",
                "parent_root": ROOT,
                "state_root": ROOT,
                "body_root": ROOT
            },
            "signature": SIGNATURE
        }
    })
}

fn create_validator_response() -> Value {
    json!({
        "index": 1,
        "balance": BALANCE.to_string(),
        "status": "active_ongoing",
        "validator": {
            "pubkey": PUBKEY_HEX,
            "withdrawal_credentials": ROOT,
            "effective_balance": BALANCE.to_string(),
            "slashed": false,
            "activation_eligibility_epoch": "0",
            "activation_epoch": "0",
            "exit_epoch": "18446744073709551615",
            "withdrawable_epoch": "18446744073709551615"
        }
    })
}

fn create_attestation() -> Value {
    json!({
        "aggregation_bits": "0x01",
        "data": {
            "slot": "1",
            "index": "1",
            "beacon_block_root": ROOT,
            "source": {
                "epoch": "1",
                "root": ROOT
            },
            "target": {
                "epoch": "1",
                "root": ROOT
            }
        },
        "signature": SIGNATURE
    })
}

fn create_checkpoint() -> Value {
    json!({
        "epoch": "1",
        "root": ROOT
    })
}

fn create_execution_meta(finalized: bool) -> Value {
    json!({
        "execution_optimistic": true,
        "finalized": finalized
    })
}

fn create_versioned_meta(version: &str, finalized: bool) -> Value {
    json!({
        "execution_optimistic": true,
        "finalized": finalized,
        "version": version
    })
}

fn create_default_block_body() -> Value {
    json!({
        "randao_reveal": SIGNATURE,
        "eth1_data": {
            "deposit_root": ROOT,
            "deposit_count": "0",
            "block_hash": ROOT
        },
        "graffiti": ROOT,
        "proposer_slashings": [],
        "attester_slashings": [],
        "attestations": [],
        "deposits": [],
        "voluntary_exits": [],
        "sync_aggregate": {
            "sync_committee_bits": "0x00",
            "sync_committee_signature": SIGNATURE
        },
        "bls_to_execution_changes": [],
        "blob_kzg_commitments": []
    })
}

fn create_execution_payload() -> Value {
    json!({
        "parent_hash": ROOT,
        "fee_recipient": "0x0000000000000000000000000000000000000000",
        "state_root": ROOT,
        "receipts_root": ROOT,
        "logs_bloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
        "prev_randao": ROOT,
        "block_number": "1",
        "gas_limit": "30000000",
        "gas_used": "0",
        "timestamp": "1606824023",
        "extra_data": "0x",
        "base_fee_per_gas": "7",
        "block_hash": ROOT,
        "transactions": [],
        "withdrawals": [],
        "blob_gas_used": "0",
        "excess_blob_gas": "0"
    })
}

fn create_execution_payload_header() -> Value {
    let mut payload = create_execution_payload();
    if let Value::Object(ref mut map) = payload {
        map.remove("transactions");
        map.remove("withdrawals");
        map.insert("transactions_root".to_string(), json!(ROOT));
        map.insert("withdrawals_root".to_string(), json!(ROOT));
    }
    payload
}

fn create_signed_block(with_execution_payload: bool) -> Value {
    let mut body = create_default_block_body();
    
    if with_execution_payload {
        if let Value::Object(ref mut body_map) = body {
            body_map.insert("execution_payload".to_string(), create_execution_payload());
        }
    }
    
    json!({
        "message": {
            "slot": "1",
            "proposer_index": "1",
            "parent_root": ROOT,
            "state_root": ROOT,
            "body": body
        },
        "signature": SIGNATURE
    })
}

fn create_signed_blinded_block() -> Value {
    let mut body = create_default_block_body();
    
    if let Value::Object(ref mut body_map) = body {
        body_map.insert("execution_payload_header".to_string(), create_execution_payload_header());
    }
    
    json!({
        "message": {
            "slot": "1",
            "proposer_index": "1",
            "parent_root": ROOT,
            "state_root": ROOT,
            "body": body
        },
        "signature": SIGNATURE
    })
}

fn create_attester_slashing() -> Value {
    json!({
        "attestation_1": {
            "attesting_indices": ["1"],
            "data": {
                "slot": "1",
                "index": "1",
                "beacon_block_root": ROOT,
                "source": create_checkpoint(),
                "target": create_checkpoint()
            },
            "signature": SIGNATURE
        },
        "attestation_2": {
            "attesting_indices": ["1"],
            "data": {
                "slot": "1",
                "index": "1",
                "beacon_block_root": ROOT,
                "source": create_checkpoint(),
                "target": create_checkpoint()
            },
            "signature": SIGNATURE
        }
    })
}

fn create_proposer_slashing() -> Value {
    json!({
        "signed_header_1": {
            "message": {
                "slot": "1",
                "proposer_index": "1",
                "parent_root": ROOT,
                "state_root": ROOT,
                "body_root": ROOT
            },
            "signature": SIGNATURE
        },
        "signed_header_2": {
            "message": {
                "slot": "1",
                "proposer_index": "1",
                "parent_root": ROOT,
                "state_root": ROOT,
                "body_root": ROOT
            },
            "signature": SIGNATURE
        }
    })
}

fn create_voluntary_exit() -> Value {
    json!({
        "message": {
            "epoch": "1",
            "validator_index": "1"
        },
        "signature": SIGNATURE
    })
}

fn create_bls_to_execution_change() -> Value {
    json!({
        "message": {
            "validator_index": "1",
            "from_bls_pubkey": PUBKEY_HEX,
            "to_execution_address": "0x0000000000000000000000000000000000000000"
        },
        "signature": SIGNATURE
    })
}

fn create_sync_committee_message() -> Value {
    json!({
        "slot": "1",
        "beacon_block_root": ROOT,
        "validator_index": "1",
        "signature": SIGNATURE
    })
}

fn create_blob_sidecar() -> Value {
    json!({
        "block_root": ROOT,
        "index": "0",
        "slot": "1",
        "block_parent_root": ROOT,
        "proposer_index": "1",
        "blob": "0x0000000000000000000000000000000000000000000000000000000000000000",
        "kzg_commitment": "0x000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
        "kzg_proof": "0x000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
    })
}

pub fn get_test_data() -> HashMap<String, Value> {
    let mut test_data = HashMap::new();

    // Block endpoints
    test_data.insert("getBlockV2".to_string(), json!({
        "args": {"blockId": "head"},
        "res": {
            "data": create_signed_block(true),
            "meta": create_versioned_meta("electra", false)
        }
    }));

    test_data.insert("getBlindedBlock".to_string(), json!({
        "args": {"blockId": "head"},
        "res": {
            "data": create_signed_blinded_block(),
            "meta": create_versioned_meta("electra", false)
        }
    }));

    test_data.insert("getBlockAttestations".to_string(), json!({
        "args": {"blockId": "head"},
        "res": {
            "data": [create_attestation()],
            "meta": create_execution_meta(false)
        }
    }));

    test_data.insert("getBlockAttestationsV2".to_string(), json!({
        "args": {"blockId": "head"},
        "res": {
            "data": [create_attestation()],
            "meta": create_versioned_meta("electra", false)
        }
    }));

    test_data.insert("getBlockHeader".to_string(), json!({
        "args": {"blockId": "head"},
        "res": {
            "data": create_block_header_response(),
            "meta": create_execution_meta(false)
        }
    }));

    test_data.insert("getBlockHeaders".to_string(), json!({
        "args": {"slot": 1, "parentRoot": ROOT},
        "res": {
            "data": [create_block_header_response()],
            "meta": create_execution_meta(false)
        }
    }));

    test_data.insert("getBlockRoot".to_string(), json!({
        "args": {"blockId": "head"},
        "res": {
            "data": {"root": ROOT},
            "meta": create_execution_meta(false)
        }
    }));

    test_data.insert("publishBlock".to_string(), json!({
        "args": {
            "signedBlockOrContents": {
                "signed_block": create_signed_block(false)
            }
        },
        "res": null
    }));

    test_data.insert("publishBlockV2".to_string(), json!({
        "args": {
            "signedBlockOrContents": {
                "signed_block": create_signed_block(false)
            },
            "broadcastValidation": "consensus"
        },
        "res": null
    }));

    test_data.insert("publishBlindedBlock".to_string(), json!({
        "args": {
            "signedBlindedBlock": create_signed_blinded_block()
        },
        "res": null
    }));

    test_data.insert("publishBlindedBlockV2".to_string(), json!({
        "args": {
            "signedBlindedBlock": create_signed_blinded_block(),
            "broadcastValidation": "consensus"
        },
        "res": null
    }));

    test_data.insert("getBlobSidecars".to_string(), json!({
        "args": {"blockId": "head", "indices": [0]},
        "res": {
            "data": [create_blob_sidecar()],
            "meta": create_versioned_meta("electra", false)
        }
    }));

    // Pool endpoints
    test_data.insert("getPoolAttestations".to_string(), json!({
        "args": {"slot": 1, "committeeIndex": 2},
        "res": {
            "data": [create_attestation()]
        }
    }));

    test_data.insert("getPoolAttestationsV2".to_string(), json!({
        "args": {"slot": 1, "committeeIndex": 2},
        "res": {
            "data": [create_attestation()],
            "meta": {"version": "electra"}
        }
    }));

    test_data.insert("getPoolAttesterSlashings".to_string(), json!({
        "args": null,
        "res": {
            "data": [create_attester_slashing()]
        }
    }));

    test_data.insert("getPoolAttesterSlashingsV2".to_string(), json!({
        "args": null,
        "res": {
            "data": [create_attester_slashing()],
            "meta": {"version": "electra"}
        }
    }));

    test_data.insert("getPoolProposerSlashings".to_string(), json!({
        "args": null,
        "res": {
            "data": [create_proposer_slashing()]
        }
    }));

    test_data.insert("getPoolVoluntaryExits".to_string(), json!({
        "args": null,
        "res": {
            "data": [create_voluntary_exit()]
        }
    }));

    test_data.insert("getPoolBLSToExecutionChanges".to_string(), json!({
        "args": null,
        "res": {
            "data": [create_bls_to_execution_change()]
        }
    }));

    test_data.insert("submitPoolAttestations".to_string(), json!({
        "args": {"signedAttestations": [create_attestation()]},
        "res": null
    }));

    test_data.insert("submitPoolAttestationsV2".to_string(), json!({
        "args": {
            "signedAttestations": [{
                "attestation": create_attestation(),
                "validator_index": "1"
            }]
        },
        "res": null
    }));

    test_data.insert("submitPoolAttesterSlashings".to_string(), json!({
        "args": {"attesterSlashing": create_attester_slashing()},
        "res": null
    }));

    test_data.insert("submitPoolAttesterSlashingsV2".to_string(), json!({
        "args": {"attesterSlashing": create_attester_slashing()},
        "res": null
    }));

    test_data.insert("submitPoolProposerSlashings".to_string(), json!({
        "args": {"proposerSlashing": create_proposer_slashing()},
        "res": null
    }));

    test_data.insert("submitPoolVoluntaryExit".to_string(), json!({
        "args": {"signedVoluntaryExit": create_voluntary_exit()},
        "res": null
    }));

    test_data.insert("submitPoolBLSToExecutionChange".to_string(), json!({
        "args": {"blsToExecutionChanges": [create_bls_to_execution_change()]},
        "res": null
    }));

    test_data.insert("submitPoolSyncCommitteeSignatures".to_string(), json!({
        "args": {"signatures": [create_sync_committee_message()]},
        "res": null
    }));

    // State endpoints
    test_data.insert("getStateRoot".to_string(), json!({
        "args": {"stateId": "head"},
        "res": {
            "data": {"root": ROOT},
            "meta": create_execution_meta(false)
        }
    }));

    test_data.insert("getStateFork".to_string(), json!({
        "args": {"stateId": "head"},
        "res": {
            "data": {
                "previous_version": "0x00000000",
                "current_version": "0x00000000",
                "epoch": "0"
            },
            "meta": create_execution_meta(false)
        }
    }));

    test_data.insert("getStateRandao".to_string(), json!({
        "args": {"stateId": "head", "epoch": 1},
        "res": {
            "data": {"randao": RANDAO},
            "meta": create_execution_meta(false)
        }
    }));

    test_data.insert("getStateFinalityCheckpoints".to_string(), json!({
        "args": {"stateId": "head"},
        "res": {
            "data": {
                "previous_justified": create_checkpoint(),
                "current_justified": create_checkpoint(),
                "finalized": create_checkpoint()
            },
            "meta": create_execution_meta(false)
        }
    }));

    test_data.insert("getStateValidators".to_string(), json!({
        "args": {
            "stateId": "head",
            "validatorIds": [PUBKEY_HEX, "1300"],
            "statuses": ["active_ongoing"]
        },
        "res": {
            "data": [create_validator_response()],
            "meta": create_execution_meta(false)
        }
    }));

    test_data.insert("postStateValidators".to_string(), json!({
        "args": {
            "stateId": "head",
            "validatorIds": [PUBKEY_HEX, 1300],
            "statuses": ["active_ongoing"]
        },
        "res": {
            "data": [create_validator_response()],
            "meta": create_execution_meta(false)
        }
    }));

    test_data.insert("postStateValidatorIdentities".to_string(), json!({
        "args": {"stateId": "head", "validatorIds": [1300]},
        "res": {
            "data": [{
                "index": 1300,
                "pubkey": PUBKEY_HEX,
                "activation_epoch": "1"
            }],
            "meta": create_execution_meta(false)
        }
    }));

    test_data.insert("getStateValidator".to_string(), json!({
        "args": {"stateId": "head", "validatorId": PUBKEY_HEX},
        "res": {
            "data": create_validator_response(),
            "meta": create_execution_meta(false)
        }
    }));

    test_data.insert("getStateValidatorBalances".to_string(), json!({
        "args": {"stateId": "head", "validatorIds": ["1300"]},
        "res": {
            "data": [{
                "index": 1300,
                "balance": BALANCE.to_string()
            }],
            "meta": create_execution_meta(false)
        }
    }));

    test_data.insert("postStateValidatorBalances".to_string(), json!({
        "args": {"stateId": "head", "validatorIds": [1300]},
        "res": {
            "data": [{
                "index": 1300,
                "balance": BALANCE.to_string()
            }],
            "meta": create_execution_meta(false)
        }
    }));

    test_data.insert("getEpochCommittees".to_string(), json!({
        "args": {"stateId": "head", "index": 1, "slot": 2, "epoch": 3},
        "res": {
            "data": [{
                "index": 1,
                "slot": 2,
                "validators": [1300]
            }],
            "meta": create_execution_meta(false)
        }
    }));

    test_data.insert("getEpochSyncCommittees".to_string(), json!({
        "args": {"stateId": "head", "epoch": 1},
        "res": {
            "data": {
                "validators": [1300],
                "validator_aggregates": [[1300]]
            },
            "meta": create_execution_meta(false)
        }
    }));

    test_data.insert("getPendingDeposits".to_string(), json!({
        "args": {"stateId": "head"},
        "res": {
            "data": [{
                "pubkey": PUBKEY_HEX,
                "withdrawal_credentials": ROOT,
                "amount": "0",
                "signature": SIGNATURE,
                "slot": "1"
            }],
            "meta": create_versioned_meta("electra", false)
        }
    }));

    test_data.insert("getPendingPartialWithdrawals".to_string(), json!({
        "args": {"stateId": "head"},
        "res": {
            "data": [{
                "index": "1",
                "amount": "0",
                "withdrawable_epoch": "1"
            }],
            "meta": create_versioned_meta("electra", false)
        }
    }));

    test_data.insert("getPendingConsolidations".to_string(), json!({
        "args": {"stateId": "head"},
        "res": {
            "data": [{
                "source_index": "1",
                "target_index": "1"
            }],
            "meta": create_versioned_meta("electra", false)
        }
    }));

    test_data.insert("getProposerLookahead".to_string(), json!({
        "args": {"stateId": "head"},
        "res": {
            "data": {
                "slot": "1",
                "proposer_index": "1"
            },
            "meta": create_versioned_meta("fulu", false)
        }
    }));

    // Rewards endpoints
    test_data.insert("getBlockRewards".to_string(), json!({
        "args": {"blockId": "head"},
        "res": {
            "data": {
                "proposer_index": 0,
                "total": 15,
                "attestations": 8,
                "sync_aggregate": 4,
                "proposer_slashings": 2,
                "attester_slashings": 1
            },
            "meta": create_execution_meta(false)
        }
    }));

    test_data.insert("getAttestationsRewards".to_string(), json!({
        "args": {"epoch": 10, "validatorIds": [1300]},
        "res": {
            "data": {
                "ideal_rewards": [{
                    "head": 0,
                    "target": 10,
                    "source": 20,
                    "inclusion_delay": 30,
                    "inactivity": 40,
                    "effective_balance": 50
                }],
                "total_rewards": [{
                    "head": 0,
                    "target": 10,
                    "source": 20,
                    "inclusion_delay": 30,
                    "inactivity": 40,
                    "validator_index": 50
                }]
            },
            "meta": create_execution_meta(false)
        }
    }));

    test_data.insert("getSyncCommitteeRewards".to_string(), json!({
        "args": {"blockId": "head", "validatorIds": [1300]},
        "res": {
            "data": [{
                "validator_index": 1300,
                "reward": REWARD
            }],
            "meta": create_execution_meta(false)
        }
    }));

    // Genesis endpoint
    test_data.insert("getGenesis".to_string(), json!({
        "args": null,
        "res": {
            "data": {
                "genesis_time": "1606824000",
                "genesis_validators_root": ROOT,
                "genesis_fork_version": "0x00000000"
            }
        }
    }));

    test_data
}