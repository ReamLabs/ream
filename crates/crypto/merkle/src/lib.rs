//! https://ethereum.github.io/consensus-specs/ssz/merkle-proofs

use alloy_primitives::B256;
use anyhow::ensure;

fn get_generalized_index_length(index: usize) -> usize {
    (index as f64).log2() as usize
}

fn get_generalized_index_bit(index: usize, position: usize) -> bool {
    (index & (1 << position)) > 0
}

fn get_generalized_index_child(index: usize, right_side: bool) -> usize {
    index * 2 + right_side as usize
}

pub fn merkle_tree(leaves: &[B256], depth: usize) -> Vec<B256> {
    let num_of_leaves = leaves.len();
    let bottom_length = 1 << depth;

    let mut tree = vec![B256::ZERO; bottom_length];
    tree.extend(leaves);
    tree.extend(vec![B256::ZERO; bottom_length - num_of_leaves]);

    for i in (1..bottom_length).rev() {
        let left = tree[i * 2].as_slice();
        let right = tree[i * 2 + 1].as_slice();
        tree[i] = ethereum_hashing::hash32_concat(left, right).into();
    }

    tree
}

pub fn generate_proof(
    tree: &[B256],
    index: usize,
    depth: usize,
) -> anyhow::Result<(B256, Vec<B256>)> {
    let mut proof = vec![];
    let mut current_index = 1;
    let mut current_depth = depth;

    while current_depth > 0 {
        let (left_child_index, right_child_index) = (
            get_generalized_index_child(current_index, false),
            get_generalized_index_child(current_index, true),
        );

        if get_generalized_index_bit(index, current_depth - 1) {
            proof.push(tree[left_child_index]);
            current_index = right_child_index;
        } else {
            proof.push(tree[right_child_index]);
            current_index = left_child_index;
        }

        current_depth -= 1;
    }

    proof.reverse();

    Ok((tree[current_index], proof))
}

pub fn calculate_merkle_root(
    leaf: B256,
    proof: Vec<B256>,
    generalized_index: usize,
) -> anyhow::Result<B256> {
    ensure!(
        proof.len() == get_generalized_index_length(generalized_index),
        "Proof length does not match index length"
    );
    let mut current = leaf;
    for (i, proof) in proof.iter().enumerate() {
        if get_generalized_index_bit(generalized_index, i) {
            current = ethereum_hashing::hash32_concat(proof.as_slice(), current.as_slice()).into();
        } else {
            current = ethereum_hashing::hash32_concat(current.as_slice(), proof.as_slice()).into();
        }
    }
    Ok(current)
}

pub fn verify_merkle_proof(
    leaf: B256,
    proof: Vec<B256>,
    index: usize,
    depth: usize,
    root: B256,
) -> anyhow::Result<bool> {
    Ok(calculate_merkle_root(leaf, proof, (1usize << depth) + index)? == root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merkle_tree() {
        let leaves = vec![
            B256::from_slice(&[0xAA; 32]),
            B256::from_slice(&[0xBB; 32]),
            B256::from_slice(&[0xCC; 32]),
            B256::from_slice(&[0xDD; 32]),
        ];

        let depth = (leaves.len() as f64).log2().ceil() as usize;

        let node_2: B256 =
            ethereum_hashing::hash32_concat(leaves[0].as_slice(), leaves[1].as_slice()).into();
        let node_3: B256 =
            ethereum_hashing::hash32_concat(leaves[2].as_slice(), leaves[3].as_slice()).into();

        let root: B256 =
            ethereum_hashing::hash32_concat(node_2.as_slice(), node_3.as_slice()).into();

        let tree = merkle_tree(&leaves, depth);

        assert_eq!(tree[1], root);

        let proof_0 = generate_proof(&tree, 0, depth).unwrap();
        let proof_1 = generate_proof(&tree, 1, depth).unwrap();
        let proof_2 = generate_proof(&tree, 2, depth).unwrap();
        let proof_3 = generate_proof(&tree, 3, depth).unwrap();

        assert_eq!(proof_0.0, leaves[0]);
        assert!(verify_merkle_proof(leaves[0], proof_0.1, 0, depth, root).unwrap());

        assert_eq!(proof_1.0, leaves[1]);
        assert!(verify_merkle_proof(leaves[1], proof_1.1, 1, depth, root).unwrap());

        assert_eq!(proof_2.0, leaves[2]);
        assert!(verify_merkle_proof(leaves[2], proof_2.1, 2, depth, root).unwrap());

        assert_eq!(proof_3.0, leaves[3]);
        assert!(verify_merkle_proof(leaves[3], proof_3.1, 3, depth, root).unwrap());
    }
}
