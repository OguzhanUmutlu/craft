pub mod dedup;
pub mod dr;
pub mod engine;
pub mod merkle;
pub mod mesh;
pub mod providers;
pub mod retention;

pub use dedup::{
    BackupManifest, ChunkRef, ChunkStore, DeduplicationEngine, FastCDC, ManifestFileEntry,
    MAX_CHUNK_SIZE, MIN_CHUNK_SIZE, TARGET_CHUNK_SIZE,
};
pub use dr::DrOrchestrator;
pub use engine::{BackupEngine, BackupFormat, BackupMetadata};
pub use merkle::{compute_merkle_root, sample_mesh_integrity, MerkleAuditReport};
pub use mesh::{MeshTargetHealth, StorageMesh};
pub use providers::*;
pub use retention::enforce_retention;
