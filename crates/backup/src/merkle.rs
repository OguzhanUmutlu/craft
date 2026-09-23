use crate::dedup::BackupManifest;
use craft_core::{CraftPaths, Result};
use hex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;

pub fn compute_merkle_root(chunk_hashes: &[String]) -> String {
    if chunk_hashes.is_empty() {
        return "0000000000000000000000000000000000000000000000000000000000000000".to_string();
    }

    let mut current_level: Vec<String> = chunk_hashes.to_vec();

    while current_level.len() > 1 {
        let mut next_level = Vec::new();
        let mut i = 0;
        while i < current_level.len() {
            let left = &current_level[i];
            let right = if i + 1 < current_level.len() {
                &current_level[i + 1]
            } else {
                // Duplicate last if odd number of leaves
                left
            };

            let mut hasher = Sha256::new();
            hasher.update(left.as_bytes());
            hasher.update(right.as_bytes());
            next_level.push(hex::encode(hasher.finalize()));

            i += 2;
        }
        current_level = next_level;
    }

    current_level[0].clone()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MerkleAuditReport {
    pub server_name: String,
    pub manifest_id: String,
    pub total_chunks: usize,
    pub sampled_chunks: usize,
    pub verified_chunks: usize,
    pub missing_chunks: Vec<String>,
    pub corrupted_chunks: Vec<String>,
    pub is_valid: bool,
}

pub fn sample_mesh_integrity(
    paths: &CraftPaths,
    manifest: &BackupManifest,
    sample_ratio: f64,
) -> Result<MerkleAuditReport> {
    let mut all_chunks = Vec::new();
    for file in &manifest.files {
        for c in &file.chunks {
            all_chunks.push(c.hash.clone());
        }
    }
    all_chunks.sort();
    all_chunks.dedup();

    let total = all_chunks.len();
    let ratio = sample_ratio.clamp(0.01, 1.0);
    let sample_count = ((total as f64) * ratio).ceil() as usize;
    let sample_count = sample_count.max(1).min(total);

    let mut missing = Vec::new();
    let mut corrupted = Vec::new();
    let mut verified = 0;

    // Deterministic pseudo-random stride to sample evenly
    let stride = if sample_count > 0 { (total / sample_count).max(1) } else { 1 };
    let mut sampled_indices = Vec::new();
    for i in (0..total).step_by(stride).take(sample_count) {
        sampled_indices.push(i);
    }

    for idx in sampled_indices {
        let chunk_hash = &all_chunks[idx];
        let prefix = if chunk_hash.len() >= 2 { &chunk_hash[..2] } else { "00" };
        let chunk_file = paths.chunks_dir.join(prefix).join(format!("{}.chunk.zst", chunk_hash));

        if !chunk_file.exists() {
            missing.push(chunk_hash.clone());
            continue;
        }

        if let Ok(meta) = fs::metadata(&chunk_file) {
            if meta.len() == 0 {
                corrupted.push(chunk_hash.clone());
            } else {
                verified += 1;
            }
        } else {
            corrupted.push(chunk_hash.clone());
        }
    }

    let is_valid = missing.is_empty() && corrupted.is_empty();

    Ok(MerkleAuditReport {
        server_name: manifest.server_name.clone(),
        manifest_id: manifest.manifest_id.clone(),
        total_chunks: total,
        sampled_chunks: sample_count,
        verified_chunks: verified,
        missing_chunks: missing,
        corrupted_chunks: corrupted,
        is_valid,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merkle_root_computation() {
        let leaves = vec![
            "aaaa0000111122223333444455556666777788889999aaaabbbbccccddddeeee".to_string(),
            "bbbb0000111122223333444455556666777788889999aaaabbbbccccddddeeee".to_string(),
            "cccc0000111122223333444455556666777788889999aaaabbbbccccddddeeee".to_string(),
        ];

        let root1 = compute_merkle_root(&leaves);
        let root2 = compute_merkle_root(&leaves);

        assert_eq!(root1, root2);
        assert_ne!(root1, compute_merkle_root(&[]));
    }
}
