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

const PARAMETER_LEN: usize = 5;
const TWEAK_LEN_FE: usize = 2;
const MSG_LEN_FE: usize = 9;
const RAND_LEN_FE: usize = 7;
const HASH_LEN_FE: usize = 8;

const CAPACITY: usize = 9;

const POS_OUTPUT_LEN_PER_INV_FE: usize = 15;
const POS_INVOCATIONS: usize = 1;
const POS_OUTPUT_LEN_FE: usize = POS_OUTPUT_LEN_PER_INV_FE * POS_INVOCATIONS;

type MH = TopLevelPoseidonMessageHash<
    POS_OUTPUT_LEN_PER_INV_FE,
    POS_INVOCATIONS,
    POS_OUTPUT_LEN_FE,
    DIMENSION,
    BASE,
    FINAL_LAYER,
    TWEAK_LEN_FE,
    MSG_LEN_FE,
    PARAMETER_LEN,
    RAND_LEN_FE,
>;
type TH = PoseidonTweakHash<PARAMETER_LEN, HASH_LEN_FE, TWEAK_LEN_FE, CAPACITY, DIMENSION>;

#[allow(clippy::upper_case_acronyms)]
type PRF = ShakePRFtoF<HASH_LEN_FE>;

type IE = TargetSumEncoding<MH, TARGET_SUM>;

pub type SIGTopLevelTargetSumLifetime8Dim16Base4 =
    GeneralizedXMSSSignatureScheme<PRF, IE, TH, LOG_LIFETIME>;
