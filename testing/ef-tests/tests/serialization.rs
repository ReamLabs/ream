use alloy_primitives::b256;
use ream_consensus::deneb::{beacon_block::SignedBeaconBlock, beacon_state::BeaconState};
use ream_rpc::types::response::BeaconVersionedResponse;

#[tokio::test]
pub async fn test_serialization() {
    let _ = test_beacon_state_serialization().await;
    let _ = test_beacon_block_serialization().await;
}

pub async fn test_beacon_state_serialization() -> anyhow::Result<()> {
    pub type Response = BeaconVersionedResponse<BeaconState>;

    println!("Serialization initiated");

    let mock_beacon_state = refactor_mock_beacon_state()?;
    let beacon_state: Response = serde_json::from_str(&mock_beacon_state).unwrap();

    assert_eq!(beacon_state.version, "deneb");
    assert_eq!(beacon_state.data.latest_block_header.slot, 1);
    assert_eq!(
        beacon_state.data.latest_block_header.parent_root,
        b256!("0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2")
    );

    println!("State Serialization completed successfully");

    Ok(())
}
pub async fn test_beacon_block_serialization() -> anyhow::Result<()> {
    pub type Response = BeaconVersionedResponse<SignedBeaconBlock>;

    println!("Block Serialization initiated");
    let beacon_block: Response = serde_json::from_str(BEACON_BLOCK).unwrap();

    assert_eq!(beacon_block.version, "deneb");
    assert_eq!(beacon_block.data.message.slot, 11532800);

    let re_encoded = serde_json::to_string(&beacon_block)?;
    let reparsed: serde_json::Value = serde_json::from_str(&re_encoded)?;
    let original: serde_json::Value = serde_json::from_str(BEACON_BLOCK)?;

    assert_eq!(
        reparsed, original,
        "Re-encoded block doesn't match original JSON"
    );

    println!("Block Serialization completed successfully");

    Ok(())
}

pub fn refactor_mock_beacon_state() -> anyhow::Result<String> {
    let mut parsed: serde_json::Value = serde_json::from_str(BEACON_STATE).unwrap();

    // add 8192 elements in `block_roots` and `state_roots`
    let dummy_root = "0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2";
    let roots: Vec<_> = std::iter::repeat_n(dummy_root, 8192)
        .map(|h| serde_json::Value::String(h.to_string()))
        .collect();
    parsed["data"]["block_roots"] = serde_json::Value::Array(roots.clone());
    parsed["data"]["state_roots"] = serde_json::Value::Array(roots);

    // add 65536 elements in `randao_mixes`.
    let randao_mix = "0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2";
    let randao_mixes: Vec<_> = std::iter::repeat_n(randao_mix, 65536)
        .map(|h| serde_json::Value::String(h.to_string()))
        .collect();
    parsed["data"]["randao_mixes"] = serde_json::Value::Array(randao_mixes.clone());

    // add 512 elements in `pubkey`.
    let dummy_pubkey = "0x93247f2209abcacf57b75a51dafae777f9dd38bc7053d1af526f220a7489a6d3a2753e5f3e8b1cfe39b56f43611df74a";
    let roots: Vec<_> = std::iter::repeat_n(dummy_pubkey, 512)
        .map(|h| serde_json::Value::String(h.to_string()))
        .collect();
    parsed["data"]["current_sync_committee"]["pubkeys"] = serde_json::Value::Array(roots.clone());
    parsed["data"]["next_sync_committee"]["pubkeys"] = serde_json::Value::Array(roots.clone());

    // add 8192 elements in `slashings`.
    let roots: Vec<_> = std::iter::repeat_n("0", 8192)
        .map(|h| serde_json::Value::String(h.to_string()))
        .collect();
    parsed["data"]["slashings"] = serde_json::Value::Array(roots.clone());

    let patched_json = serde_json::to_string(&parsed)?;

    Ok(patched_json)
}

static BEACON_STATE: &str = r#"{
    "version": "deneb",
    "execution_optimistic": false,
    "finalized": false,
    "data": {
      "genesis_time": "1",
      "genesis_validators_root": "0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2",
      "slot": "1",
      "fork": {
        "previous_version": "0x00000000",
        "current_version": "0x00000000",
        "epoch": "1"
      },
      "latest_block_header": {
        "slot": "1",
        "proposer_index": "1",
        "parent_root": "0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2",
        "state_root": "0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2",
        "body_root": "0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2"
      },
      "block_roots": [],
      "state_roots": [],
      "historical_roots": [
        "0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2"
      ],
      "eth1_data": {
        "deposit_root": "0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2",
        "deposit_count": "1",
        "block_hash": "0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2"
      },
      "eth1_data_votes": [
        {
          "deposit_root": "0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2",
          "deposit_count": "1",
          "block_hash": "0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2"
        }
      ],
      "eth1_deposit_index": "1",
      "validators": [
        {
          "pubkey": "0x93247f2209abcacf57b75a51dafae777f9dd38bc7053d1af526f220a7489a6d3a2753e5f3e8b1cfe39b56f43611df74a",
          "withdrawal_credentials": "0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2",
          "effective_balance": "1",
          "slashed": false,
          "activation_eligibility_epoch": "1",
          "activation_epoch": "1",
          "exit_epoch": "1",
          "withdrawable_epoch": "1"
        }
      ],
      "balances": [
        "1"
      ],
      "randao_mixes": [
        "0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2"
      ],
      "slashings": [
        "1"
      ],
      "previous_epoch_participation": [
        "0"
      ],
      "current_epoch_participation": [
        "0"
      ],
      "justification_bits": "0x01",
      "previous_justified_checkpoint": {
        "epoch": "1",
        "root": "0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2"
      },
      "current_justified_checkpoint": {
        "epoch": "1",
        "root": "0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2"
      },
      "finalized_checkpoint": {
        "epoch": "1",
        "root": "0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2"
      },
      "inactivity_scores": [
        "1"
      ],
      "current_sync_committee": {
        "pubkeys": [
          "0x93247f2209abcacf57b75a51dafae777f9dd38bc7053d1af526f220a7489a6d3a2753e5f3e8b1cfe39b56f43611df74a"
        ],
        "aggregate_pubkey": "0x93247f2209abcacf57b75a51dafae777f9dd38bc7053d1af526f220a7489a6d3a2753e5f3e8b1cfe39b56f43611df74a"
      },
      "next_sync_committee": {
        "pubkeys": [
          "0x93247f2209abcacf57b75a51dafae777f9dd38bc7053d1af526f220a7489a6d3a2753e5f3e8b1cfe39b56f43611df74a"
        ],
        "aggregate_pubkey": "0x93247f2209abcacf57b75a51dafae777f9dd38bc7053d1af526f220a7489a6d3a2753e5f3e8b1cfe39b56f43611df74a"
      },
      "latest_execution_payload_header": {
        "parent_hash": "0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2",
        "fee_recipient": "0xAbcF8e0d4e9587369b2301D0790347320302cc09",
        "state_root": "0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2",
        "receipts_root": "0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2",
        "logs_bloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
        "prev_randao": "0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2",
        "block_number": "1",
        "gas_limit": "1",
        "gas_used": "1",
        "timestamp": "1",
        "extra_data": "0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2",
        "base_fee_per_gas": "1",
        "blob_gas_used": "1",
        "excess_blob_gas": "1",
        "block_hash": "0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2",
        "transactions_root": "0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2",
        "withdrawals_root": "0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2"
      },
      "next_withdrawal_index": "1",
      "next_withdrawal_validator_index": "1",
      "historical_summaries": [
        {
          "block_summary_root": "0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2",
          "state_summary_root": "0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2"
        }
      ]
    }
  }"#;

static BEACON_BLOCK: &str = r#"{
  "version": "deneb",
  "execution_optimistic": false,
  "finalized": true,
  "data": {
    "message": {
      "slot": "11532800",
      "proposer_index": "231554",
      "parent_root": "0x81c89d2dbd540ade21b9c28d8a78395706563ac5af78fb67c3960ecad3706c8a",
      "state_root": "0x78329cf91573da18accec7f7eb665a482dff15a8134b71b2a6c4b79cc5d051c3",
      "body": {
        "randao_reveal": "0xb2b1cf51795c2993650803cf87bf9ae07014814ebea049b8c91046711acace0d0bc60d4dc44037d8938a322bc74f83b109ab7d8bce95166ed5bd45f25b71780d84e649f76bba6ea88e41b4a60bc7d10cfea7dd16b45158dc87dcfe9feb8b8bb5",
        "eth1_data": {
          "deposit_root": "0x57e20ae33b9ae2ca7fa09d41f3c0f40894c766c2ca7107e34575e5ad44891456",
          "deposit_count": "2024751",
          "block_hash": "0xa6a7af9c65ed8604e270d25b73cd6ba0b0cae490112643ed0478af8ec4fdb4c2"
        },
        "graffiti": "0x4e4d383763384c48306439300000000000000000000000000000000000000000",
        "proposer_slashings": [],
        "attester_slashings": [],
        "attestations": [
          {
            "aggregation_bits": "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff03",
            "data": {
              "slot": "11532799",
              "index": "13",
              "beacon_block_root": "0x81c89d2dbd540ade21b9c28d8a78395706563ac5af78fb67c3960ecad3706c8a",
              "source": {
                "epoch": "360398",
                "root": "0xa64cd7205b4da157da0b4ad3de9c94eb62bb308753232f644f580d77c3c3af71"
              },
              "target": {
                "epoch": "360399",
                "root": "0xb80876444cbed546b55b6e303ba51ee1f3358cc83f7d351fd4a8fe601a08c268"
              }
            },
            "signature": "0xb5119f435c9e8cd7e1162e82cf9c7d202c1dce7ce1973c744b1e0f3fe632e3f38383f8a8b66c196c277662a6a6589f3716326d0e8cb2d3fb8d974aaa48e30cb83ea9ee66d7714ba30f9675e71967a92afc372c074da9d112685a7f6ab27e5b65"
          },
          {
            "aggregation_bits": "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff03",
            "data": {
              "slot": "11532799",
              "index": "43",
              "beacon_block_root": "0x81c89d2dbd540ade21b9c28d8a78395706563ac5af78fb67c3960ecad3706c8a",
              "source": {
                "epoch": "360398",
                "root": "0xa64cd7205b4da157da0b4ad3de9c94eb62bb308753232f644f580d77c3c3af71"
              },
              "target": {
                "epoch": "360399",
                "root": "0xb80876444cbed546b55b6e303ba51ee1f3358cc83f7d351fd4a8fe601a08c268"
              }
            },
            "signature": "0x801eece95d558c7e36e4ed4be26016bef52efb57eb4bc8afeaa196be9677362d722f34cad12fb405a6e0d43b3890ead7126a8202636c786afea965edf9ec12a0907299f3851fa3f54ea200c16b28fef6ab4728b363cfb243cb097e70db8854ea"
          },
          {
            "aggregation_bits": "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff03",
            "data": {
              "slot": "11532799",
              "index": "20",
              "beacon_block_root": "0x81c89d2dbd540ade21b9c28d8a78395706563ac5af78fb67c3960ecad3706c8a",
              "source": {
                "epoch": "360398",
                "root": "0xa64cd7205b4da157da0b4ad3de9c94eb62bb308753232f644f580d77c3c3af71"
              },
              "target": {
                "epoch": "360399",
                "root": "0xb80876444cbed546b55b6e303ba51ee1f3358cc83f7d351fd4a8fe601a08c268"
              }
            },
            "signature": "0x819b8b9b872c7866576f29ee4e24a4d32c89e920b7c22fa413128db1a6b81cea57fb9bf99582b001d745e9ff5215b09119b8c6b6c1d432f6b00beb9fe02d9d683ed3f45f2a938943f764a0bcd5d6b88a0e273dc223d3d5984386aae13f65e53c"
          }
        ],
        "deposits": [],
        "voluntary_exits": [],
        "sync_aggregate": {
          "sync_committee_bits": "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
          "sync_committee_signature": "0x85b0b2a80f322e7462e45a0f9efd8aa8d8c61a052a1d335614db616160f108e9170720880f286d1a9295286fd8d3840a1039cf0ea4a607baa463ad1f48bf2946258a38baabd53f3faf78b7625a736cd8998be103ce26d090d4f5dab43107564d"
        },
        "execution_payload": {
          "parent_hash": "0x465d87b36de78d4d622e5d9ef1c2c14f896e187ef6eebcb9c042c97fb18c8485",
          "fee_recipient": "0x396343362be2a4da1ce0c1c210945346fb82aa49",
          "state_root": "0xdd023fce897cf90816ed1c54a26965134d848d914cb244e460e7d022425fb78a",
          "receipts_root": "0xb003f72022f5207a6a7b5f6755391a6e56bd563f09c8a1456d00cde599f51a9e",
          "logs_bloom": "0x2102046181001b0000148e5400043546408135004811410800884040409080020034a000008200801841024088843128060008008800204400108222486004488a00e82000c201395801421880206020000d000412500c00800000058202040052c404001200125102030206211208180d0008c82880361090c04010480ba4420208002802224084100880140b1521160241880147201202009a8004c834b8081200228109922802086900a1400380120288821880080050000804e0028003420201c60208640840850a402a1054088480c40004400c02028820370a202022400830a1482204040809000210004089438445000058c04241520b000208400000",
          "prev_randao": "0x937766d58929b5ee4ef89d49f31ef0eaa890105b5aba76b030419a35f57ffceb",
          "block_number": "22315643",
          "gas_limit": "35999895",
          "gas_used": "5002300",
          "timestamp": "1745217623",
          "extra_data": "0xe29ca82051756173617220287175617361722e77696e2920e29ca8",
          "base_fee_per_gas": "346220230",
          "block_hash": "0x1d3e3eb61ef699c48d3a05b5834f07de629ad5090a91589bde981fac0db65674",
          "transactions": [
            "0x02f901520182242284aa78230084aa7823008302d6699468b3465833fb72a70ecdf485e0e4c7bd8665fc4580b8e404e45aaf000000000000000000000000168e209d7b2f58f1f24b8ae7b7d35e662bbf11cc000000000000000000000000dac17f958d2ee523a2206206994597c13d831ec70000000000000000000000000000000000000000000000000000000000002710000000000000000000000000bbb5008da7ef90a416a4389fdad3872d6896cff900000000000000000000000000000000000000000000102b859769ac0ae600000000000000000000000000000000000000000000000000000000000006d3fda90000000000000000000000000000000000000000000000000000000000000000c080a047349d2a344a6d10319f5bbec1663cb446a0dc20c788e84c417dda4c8c3bb028a0398ab564931aca0bc85cae14ff2b92e2b2322274d5a0074d1054b5ae056b300d",
            "0x02f8d2018304a5168459682f008483cf87fe82f2089446950ba8946d7be4594399bcf203fb53e1fd7d3780b8648f975a6400000000000000000000000064bc2ca1be492be7185faa2c8835d9b824c8a194000000000000000000000000410f4d628cfc6e788b54206022cc01341d6166d800000000000000000000000000000000000000000000000a7669bd7132640000c080a00bfb617f48517a2afddeaebfa67d88e55cce68b46ef5f329c1e5416ebc876a80a04d8b9eceeb4858489f5792a56287f6e14f463c40096497687f50101064622b92"
          ],
          "withdrawals": [
            {
              "index": "84490907",
              "validator_index": "253071",
              "address": "0x9f4cf329f4cf376b7aded854d6054859dd102a2a",
              "amount": "19091632"
            }
          ],
          "blob_gas_used": "786432",
          "excess_blob_gas": "63700992"
        },
        "bls_to_execution_changes": [],
        "blob_kzg_commitments": [
          "0x82fb14e2d1c4262498be8c164219ebc4949dbbb8bd7b626fac4422a4408b20569989f4579367ef6433a7d83d0addf800"
        ]
      }
    },
    "signature": "0x96e3ac0f837f8210b1eb8caff790eef4111d0e28ef1d1600e418c1451916340e88b8176a864f64a7decdf13c839e677b04d3ebb219c21b95c3e57f1f55f7c097f07736b7c8961e3fee9574793f8329494b8902143fa6b1de3dd1717178e1f506"
  }
}
"#;
