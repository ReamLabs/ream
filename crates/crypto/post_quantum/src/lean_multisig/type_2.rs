use anyhow::{Result, anyhow};
use lean_multisig_type2::{
    F, MESSAGE_LEN_FE, MultiMessageAggregateSignature, SingleMessageAggregateSignature,
    XmssPublicKey, XmssSignature, aggregate_single_msg_signatures, merge_single_message_aggregates,
    setup_prover, split_multi_message_aggregate, verify_multi_message_aggregate,
    verify_single_message_aggregate,
};

pub const LOG_INV_RATE: usize = 2;

pub fn type_2_setup() {
    setup_prover();
}

fn bytes32_to_message(bytes: &[u8; 32]) -> [F; MESSAGE_LEN_FE] {
    std::array::from_fn(|i| {
        let chunk: [u8; 4] = bytes[i * 4..(i + 1) * 4].try_into().unwrap();
        F::new(u32::from_le_bytes(chunk))
    })
}

pub fn type_1_from_wire(wire: &[u8]) -> Result<SingleMessageAggregateSignature> {
    SingleMessageAggregateSignature::decompress(wire).ok_or_else(|| {
        anyhow!("Failed to decode single-message aggregate multi-signature from wire bytes")
    })
}

pub fn type_1_to_wire(proof: &SingleMessageAggregateSignature) -> Vec<u8> {
    proof.compress()
}

pub fn type_1_aggregate(
    children: &[SingleMessageAggregateSignature],
    raw_xmss: Vec<(XmssPublicKey, XmssSignature)>,
    message: &[u8; 32],
    slot: u32,
) -> Result<SingleMessageAggregateSignature> {
    type_2_setup();
    let message_fe = bytes32_to_message(message);
    aggregate_single_msg_signatures(children, raw_xmss, message_fe, slot, LOG_INV_RATE)
        .map_err(|err| anyhow!("single-message aggregate aggregation failed: {err:?}"))
}

pub fn type_1_verify(proof: &SingleMessageAggregateSignature) -> Result<()> {
    type_2_setup();
    verify_single_message_aggregate(proof)
        .map(|_| ())
        .map_err(|err| anyhow!("single-message aggregate verification failed: {err:?}"))
}

pub fn type_2_merge(
    parts: Vec<SingleMessageAggregateSignature>,
) -> Result<MultiMessageAggregateSignature> {
    type_2_setup();
    merge_single_message_aggregates(parts, LOG_INV_RATE)
        .map_err(|err| anyhow!("multi-message aggregate merge failed: {err:?}"))
}

pub fn type_2_to_wire(proof: &MultiMessageAggregateSignature) -> Vec<u8> {
    proof.compress()
}

pub fn type_2_from_wire(wire: &[u8]) -> Result<MultiMessageAggregateSignature> {
    MultiMessageAggregateSignature::decompress(wire).ok_or_else(|| {
        anyhow!("Failed to decode multi-message aggregate multi-signature from wire bytes")
    })
}

pub fn type_2_verify(proof: &MultiMessageAggregateSignature) -> Result<()> {
    type_2_setup();
    verify_multi_message_aggregate(proof)
        .map(|_| ())
        .map_err(|err| anyhow!("multi-message aggregate verification failed: {err:?}"))
}

pub fn type_2_split(
    proof: MultiMessageAggregateSignature,
    index: usize,
) -> Result<SingleMessageAggregateSignature> {
    type_2_setup();
    split_multi_message_aggregate(proof, index, LOG_INV_RATE)
        .map_err(|err| anyhow!("multi-message aggregate split failed: {err:?}"))
}

pub fn type_2_verify_block(
    wire: &[u8],
    expected_bindings: &[([u8; 32], u32)],
) -> Result<()> {
    type_2_setup();

    let proof = type_2_from_wire(wire)?;

    if proof.info.len() != expected_bindings.len() {
        return Err(anyhow!(
            "Block proof has {} components but {} bindings were expected",
            proof.info.len(),
            expected_bindings.len()
        ));
    }

    for (component, (message, slot)) in proof.info.iter().zip(expected_bindings.iter()) {
        let expected_message = bytes32_to_message(message);
        if component.message != expected_message {
            return Err(anyhow!(
                "Block proof component message does not match block body"
            ));
        }
        if component.slot != *slot {
            return Err(anyhow!(
                "Block proof component slot does not match block body"
            ));
        }
    }

    verify_multi_message_aggregate(&proof)
        .map(|_| ())
        .map_err(|err| anyhow!("multi-message aggregate verification failed: {err:?}"))
}
