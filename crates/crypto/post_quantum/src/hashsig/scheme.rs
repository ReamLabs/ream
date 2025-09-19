use hashsig::{
    inc_encoding::target_sum::TargetSumEncoding,
    signature::generalized_xmss::GeneralizedXMSSSignatureScheme,
    symmetric::{
        message_hash::top_level_poseidon::TopLevelPoseidonMessageHash,
        prf::shake_to_field::ShakePRFtoF, tweak_hash::poseidon::PoseidonTweakHash,
    },
};

// TEST_CONFIG signature scheme parameters based on leanSpec configuration
// Source: https://github.com/leanEthereum/leanSpec/blob/a2bc45b66b1fa8506dfae54f9966563d1e54101c/src/lean_spec/subspecs/xmss/constants.py#L121-L137

const LOG_LIFETIME: usize = 8;
const DIMENSION: usize = 16;
const BASE: usize = 4;
const FINAL_LAYER: usize = 24;
const TARGET_SUM: usize = 24;

const PARAMETER_LENGTH: usize = 5;
const TWEAK_LENGTH_FIELD_ELEMENTS: usize = 2;
const MESSAGE_LENGTH_FIELD_ELEMENTS: usize = 9;
const RAND_LENGTH_FIELD_ELEMENTS: usize = 7;
const HASH_LENGTH_FIELD_ELEMENTS: usize = 8;

const CAPACITY: usize = 9;

const POSEIDON_OUTPUT_LENGTH_PER_INVOCATION_FIELD_ELEMENTS: usize = 15;
const POSEIDON_INVOCATIONS: usize = 1;
const POSEIDON_OUTPUT_LENGTH_FIELD_ELEMENTS: usize =
    POSEIDON_OUTPUT_LENGTH_PER_INVOCATION_FIELD_ELEMENTS * POSEIDON_INVOCATIONS;

type MessageHash = TopLevelPoseidonMessageHash<
    POSEIDON_OUTPUT_LENGTH_PER_INVOCATION_FIELD_ELEMENTS,
    POSEIDON_INVOCATIONS,
    POSEIDON_OUTPUT_LENGTH_FIELD_ELEMENTS,
    DIMENSION,
    BASE,
    FINAL_LAYER,
    TWEAK_LENGTH_FIELD_ELEMENTS,
    MESSAGE_LENGTH_FIELD_ELEMENTS,
    PARAMETER_LENGTH,
    RAND_LENGTH_FIELD_ELEMENTS,
>;
type TweakableHash = PoseidonTweakHash<
    PARAMETER_LENGTH,
    HASH_LENGTH_FIELD_ELEMENTS,
    TWEAK_LENGTH_FIELD_ELEMENTS,
    CAPACITY,
    DIMENSION,
>;

#[allow(clippy::upper_case_acronyms)]
type PseudoRandomFunction = ShakePRFtoF<HASH_LENGTH_FIELD_ELEMENTS>;

type IncomparableEncoding = TargetSumEncoding<MessageHash, TARGET_SUM>;

pub type SIGTopLevelTargetSumLifetime8Dim16Base4 = GeneralizedXMSSSignatureScheme<
    PseudoRandomFunction,
    IncomparableEncoding,
    TweakableHash,
    LOG_LIFETIME,
>;
