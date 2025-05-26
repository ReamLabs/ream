//! https://ethereum.github.io/consensus-specs/ssz/merkle-proofs/#merkle-multiproofs

use std::collections::{BTreeMap, HashMap};

use alloy_primitives::B256;
use anyhow::{anyhow, ensure};
use serde::{Deserialize, Serialize};

use crate::{
    hash_concat,
    index::{
        generalized_index_from_leaf_index, generalized_index_parent, generalized_index_sibling,
        get_helper_indices,
    },
};

/// Multiproof is a structure that contains the leaves to be verified with
/// their corresponding proofs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Multiproof {
    /// The leaves to be verified.
    /// Keyed by their generalized indices.
    pub leaves: HashMap<u64, B256>,

    /// The proof nodes.
    /// Keyed by their generalized indices.
    /// Keys of ``proofs`` will be sorted in descending order when generating a single proof.
    pub proofs: BTreeMap<u64, B256>,
}

impl Multiproof {
    /// Generate a multiproof for the given tree and indices.
    pub fn generate<const DEPTH: u64>(tree: &[B256], indices: &[u64]) -> anyhow::Result<Self> {
        let bottom_length: u64 = 1 << DEPTH;

        ensure!(!indices.is_empty(), "Indices cannot be empty");
        ensure!(
            indices.iter().all(|&index| index < bottom_length),
            "Index out of bounds"
        );
        let generalized_indices = indices
            .iter()
            .map(|&index| generalized_index_from_leaf_index(index, DEPTH))
            .collect::<Vec<_>>();
        let helper_indices = get_helper_indices(&generalized_indices);

        let leaves = generalized_indices
            .iter()
            .map(|&generalized_index| (generalized_index, tree[generalized_index as usize]))
            .collect::<HashMap<u64, B256>>();
        let proofs = helper_indices
            .iter()
            .map(|&generalized_index| (generalized_index, tree[generalized_index as usize]))
            .collect::<BTreeMap<u64, B256>>();

        Ok(Self { leaves, proofs })
    }

    /// Return the root of the multiproof.
    pub fn calculate_root(&self) -> anyhow::Result<B256> {
        let leaf_indices = self.leaves.keys().cloned().collect::<Vec<_>>();
        let helper_indices = get_helper_indices(&leaf_indices);

        ensure!(
            self.proofs.len() == helper_indices.len(),
            "Invalid proof: proof length ({}) does not match helper indices length ({})",
            self.proofs.len(),
            helper_indices.len(),
        );

        // ``objects`` is a map of all the indices to their corresponding nodes (hash values).
        let mut objects = HashMap::new();
        for (index, node) in self.leaves.iter().chain(self.proofs.iter()) {
            objects.insert(*index, *node);
        }

        let mut keys = objects.keys().cloned().collect::<Vec<_>>();
        // Sort in descending order
        keys.sort_by(|a, b| b.cmp(a));

        let mut position = 0;
        while position < keys.len() {
            let key = keys
                .get(position)
                .ok_or_else(|| anyhow!("Missing key at position {position}"))?;
            let parent_index = generalized_index_parent(*key);

            let key_present = objects.contains_key(key);
            let sibling_present = objects.contains_key(&generalized_index_sibling(*key));
            let parent_missing = !objects.contains_key(&parent_index);

            if key_present && sibling_present && parent_missing {
                let right_index = key | 1;
                let left_index = generalized_index_sibling(right_index);
                let left_input = objects
                    .get(&left_index)
                    .ok_or_else(|| anyhow!("Missing left node at index {left_index}"))?;
                let right_input = objects
                    .get(&right_index)
                    .ok_or_else(|| anyhow!("Missing right node at index {right_index}"))?;

                *objects.entry(parent_index).or_default() =
                    hash_concat(left_input.as_slice(), right_input.as_slice());
                keys.push(parent_index);
            }
            position += 1;
        }

        let root = *objects
            .get(&1)
            .ok_or_else(|| anyhow!("Missing root node at index 1"))?;
        Ok(root)
    }

    /// Verify the multiproof against the given root.
    pub fn verify(&self, expected_root: B256) -> anyhow::Result<()> {
        let calculated_root = self.calculate_root()?;
        ensure!(
            calculated_root == expected_root,
            "Invalid proof: expected root {expected_root:?}, but got {calculated_root:?}"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merkle_tree;

    const DEPTH: u64 = 3;

    #[test]
    fn test_generate_and_verify_multiproof() {
        let leaves = vec![
            B256::from_slice(&[0xAA; 32]),
            B256::from_slice(&[0xBB; 32]),
            B256::from_slice(&[0xCC; 32]),
            B256::from_slice(&[0xDD; 32]),
            B256::from_slice(&[0xEE; 32]),
            B256::from_slice(&[0xFF; 32]),
            B256::from_slice(&[0x11; 32]),
            B256::from_slice(&[0x22; 32]),
        ];
        let tree = merkle_tree(&leaves, DEPTH).unwrap();

        let target_indices = vec![2, 7];
        let multiproof = Multiproof::generate::<DEPTH>(&tree, &target_indices).unwrap();

        let root = tree[1];

        assert_eq!(multiproof.leaves.len(), target_indices.len());

        // We need four nodes to prove those two leaves.
        // See this [illustration](https://hackmd.io/_uploads/H1ZVOVille.png).
        assert_eq!(multiproof.proofs.len(), 4);

        // Should succeed to verify the multiproof.
        multiproof.verify(root).unwrap();
    }
}
