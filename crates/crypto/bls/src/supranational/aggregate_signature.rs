use blst::min_pk::AggregateSignature as BlstAggregateSignature;

use crate::{
    aggregate_signature::AggregateSignature,
    signature::BLSSignature,
    traits::{Aggregatable, SupranationalAggregatable},
};

impl Aggregatable<BLSSignature> for AggregateSignature {
    type Error = anyhow::Error;
    type Output = AggregateSignature;

    fn aggregate(signatures: &[&BLSSignature]) -> anyhow::Result<Self::Output> {
        let blst_signatures = signatures
            .iter()
            .map(|pk| pk.to_blst_signature())
            .collect::<Result<Vec<_>, _>>()?;
        let aggregate_signature =
            BlstAggregateSignature::aggregate(&blst_signatures.iter().collect::<Vec<_>>(), true)
                .map_err(|err| {
                    anyhow::anyhow!("Failed to aggregate and validate signatures {err:?}")
                })?;
        Ok(Self {
            inner: aggregate_signature.to_signature().into(),
        })
    }
}

impl SupranationalAggregatable<BLSSignature, AggregateSignature> for AggregateSignature {}
