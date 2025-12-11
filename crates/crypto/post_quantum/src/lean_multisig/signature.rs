use core::convert::TryInto;

use anyhow::anyhow;
use lean_multisig::{F, PrimeCharacteristicRing};
use xmss::{V, WotsSignature, XmssSignature};

use crate::lean_multisig::errors::LeanMultisigError::{self, SerializationError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub inner: Vec<u8>,
}

impl Signature {
    pub fn new(inner: Vec<u8>) -> Self {
        Self { inner }
    }

    pub fn from_xmss(xmss_signature: &XmssSignature) -> Result<Self, LeanMultisigError> {
        let mut serialized = Vec::new();

        // Serialize randomness
        for element in &xmss_signature.wots_signature.randomness {
            let value: u32 = format!("{element:?}").parse().map_err(|err| {
                SerializationError(anyhow!("Failed to parse randomness field element: {err}"))
            })?;
            serialized.extend_from_slice(&value.to_le_bytes());
        }

        // Serialize chain_tips
        for digest in &xmss_signature.wots_signature.chain_tips {
            for field in digest {
                let value: u32 = format!("{field:?}").parse().map_err(|err| {
                    SerializationError(anyhow!("Failed to parse chain tips field element: {err}"))
                })?;
                serialized.extend_from_slice(&value.to_le_bytes());
            }
        }

        // Serialize slot
        serialized.extend_from_slice(&xmss_signature.slot.to_le_bytes());

        // Serialize merkle_proof length
        let merkle_proof_len = xmss_signature.merkle_proof.len() as u32;
        serialized.extend_from_slice(&merkle_proof_len.to_le_bytes());

        // Serialize merkle_proof data
        for digest in &xmss_signature.merkle_proof {
            for field in digest {
                let value: u32 = format!("{field:?}").parse().map_err(|err| {
                    SerializationError(anyhow!("Failed to parse Merkle proof field element: {err}"))
                })?;
                serialized.extend_from_slice(&value.to_le_bytes());
            }
        }

        Ok(Self::new(serialized))
    }

    pub fn as_xmss(&self) -> anyhow::Result<XmssSignature> {
        let bytes = self.inner.as_slice();
        let mut offset = 0;

        // Deserialize randomness
        let mut randomness = [F::from_usize(0); 8];
        for (i, chunk) in bytes[offset..offset + 32].chunks(4).enumerate() {
            let value = u32::from_le_bytes(chunk.try_into()?);
            randomness[i] = F::from_usize(value as usize);
        }
        offset += 32;

        // Deserialize chain_tips
        let mut chain_tips_vector: Vec<[F; 8]> = Vec::with_capacity(V);
        for _ in 0..V {
            let mut digest = [F::from_usize(0); 8];
            for (i, chunk) in bytes[offset..offset + 32].chunks(4).enumerate() {
                let value = u32::from_le_bytes(chunk.try_into()?);
                digest[i] = F::from_usize(value as usize);
            }
            offset += 32;
            chain_tips_vector.push(digest);
        }

        let chain_tips: [[F; 8]; V] = chain_tips_vector
            .try_into()
            .map_err(|err| anyhow!("Incorrect number of chain tips read for V. {err:?}"))?;

        // Deserialize slot
        let slot = u64::from_le_bytes(bytes[offset..offset + 8].try_into()?);
        offset += 8;

        // Deserialize merkle_proof length
        let merkle_proof_len_bytes: [u8; 4] = bytes[offset..offset + 4].try_into()?;
        let merkle_proof_len = u32::from_le_bytes(merkle_proof_len_bytes) as usize;
        offset += 4;

        // Deserialize merkle_proof data
        let mut merkle_proof = Vec::with_capacity(merkle_proof_len);
        for _ in 0..merkle_proof_len {
            let mut digest = [F::from_usize(0); 8];
            for (i, chunk) in bytes[offset..offset + 32].chunks(4).enumerate() {
                let value = u32::from_le_bytes(chunk.try_into()?);
                digest[i] = F::from_usize(value as usize);
            }
            offset += 32;
            merkle_proof.push(digest);
        }

        Ok(XmssSignature {
            wots_signature: WotsSignature {
                randomness,
                chain_tips,
            },
            slot,
            merkle_proof,
        })
    }
}
