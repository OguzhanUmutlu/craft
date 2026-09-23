pub mod chunk_cache;
pub mod io_uring_driver;
pub mod region;
pub mod registry;

pub use chunk_cache::{
    AnvilCacheStats, AnvilChunkCache, CachedChunk, ChunkKey, PrefetchSummary,
};
pub use io_uring_driver::{
    AnvilIoEngine, IoBatchRead, IoBatchWrite, IoEngineType, IoReadResult, IoWriteResult,
};
pub use region::{
    chunk_coords_to_index, compress_payload, decompress_payload, region_coords_from_chunk,
    region_filename, ChunkCompressionScheme, ChunkData, ChunkLocation, CompactionStats,
    RegionFileReader, RegionFileWriter, RegionFileHeader, RegionInspection, HEADER_BYTES,
    HEADER_SECTORS, SECTOR_BYTES, TOTAL_CHUNKS,
};
pub use registry::{
    AnvilBenchmarkReport, AnvilConfig, AnvilRegistry, AnvilStatusSummary, ChunkSectorInfo,
    RegionDetails,
};

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_chunk_coords_and_indexing() {
        assert_eq!(chunk_coords_to_index(0, 0), 0);
        assert_eq!(chunk_coords_to_index(1, 0), 1);
        assert_eq!(chunk_coords_to_index(31, 0), 31);
        assert_eq!(chunk_coords_to_index(0, 1), 32);
        assert_eq!(chunk_coords_to_index(31, 31), 1023);

        // Negative coordinates wrap correctly in 32x32 region
        assert_eq!(region_coords_from_chunk(0, 0), (0, 0));
        assert_eq!(region_coords_from_chunk(31, 31), (0, 0));
        assert_eq!(region_coords_from_chunk(32, 0), (1, 0));
        assert_eq!(region_coords_from_chunk(-1, -1), (-1, -1));
        assert_eq!(region_filename(35, 65), "r.1.2.mca");
    }

    #[test]
    fn test_region_file_roundtrip_and_compression() {
        let dir = tempdir().unwrap();
        let region_path = dir.path().join("r.0.0.mca");

        // Create writer and write chunks
        {
            let mut writer = RegionFileWriter::open_or_create(&region_path).unwrap();
            let dummy_nbt_chunk0 = b"TAG_Compound: { Level: { xPos: 0, zPos: 0, test_blocks: [1,2,3,4] } }".to_vec();
            let chunk0 = ChunkData {
                chunk_x: 0,
                chunk_z: 0,
                timestamp: 1700000000,
                scheme: ChunkCompressionScheme::Zlib,
                raw_payload: dummy_nbt_chunk0.clone(),
                compressed_size: 0,
            };
            writer.write_chunk(&chunk0).unwrap();

            let dummy_nbt_chunk1 = b"TAG_Compound: { Level: { xPos: 1, zPos: 1, test_blocks: [5,6,7,8] } }".to_vec();
            let chunk1 = ChunkData {
                chunk_x: 1,
                chunk_z: 1,
                timestamp: 1700000005,
                scheme: ChunkCompressionScheme::Zstd,
                raw_payload: dummy_nbt_chunk1.clone(),
                compressed_size: 0,
            };
            writer.write_chunk(&chunk1).unwrap();
        }

        // Open reader and verify contents
        {
            let mut reader = RegionFileReader::open(&region_path).unwrap();
            assert_eq!(reader.header().active_chunks_count(), 2);

            let read_c0 = reader.read_chunk(0, 0).unwrap().expect("chunk (0,0) must exist");
            assert_eq!(read_c0.chunk_x, 0);
            assert_eq!(read_c0.chunk_z, 0);
            assert_eq!(read_c0.scheme, ChunkCompressionScheme::Zlib);
            assert_eq!(read_c0.raw_payload, b"TAG_Compound: { Level: { xPos: 0, zPos: 0, test_blocks: [1,2,3,4] } }");

            let read_c1 = reader.read_chunk(1, 1).unwrap().expect("chunk (1,1) must exist");
            assert_eq!(read_c1.chunk_x, 1);
            assert_eq!(read_c1.chunk_z, 1);
            assert_eq!(read_c1.scheme, ChunkCompressionScheme::Zstd);
            assert_eq!(read_c1.raw_payload, b"TAG_Compound: { Level: { xPos: 1, zPos: 1, test_blocks: [5,6,7,8] } }");

            // Inspect region
            let inspection = reader.inspect().unwrap();
            assert_eq!(inspection.active_chunks, 2);
            assert_eq!(inspection.empty_chunks, 1022);
            assert!(inspection.file_size_bytes >= 8192);
        }

        // Test compaction / defragmentation
        {
            let mut writer = RegionFileWriter::open_or_create(&region_path).unwrap();
            let stats = writer.compact().unwrap();
            assert_eq!(stats.chunks_migrated, 2);
        }
    }

    #[test]
    fn test_io_engine_and_cache_prefetch() {
        let dir = tempdir().unwrap();
        let region_dir = dir.path().join("region");
        std::fs::create_dir_all(&region_dir).unwrap();
        let region_path = region_dir.join("r.0.0.mca");

        // Write some chunks
        {
            let mut writer = RegionFileWriter::open_or_create(&region_path).unwrap();
            for cz in 0..4 {
                for cx in 0..4 {
                    let chunk = ChunkData {
                        chunk_x: cx,
                        chunk_z: cz,
                        timestamp: 1700000000,
                        scheme: ChunkCompressionScheme::Zlib,
                        raw_payload: format!("Payload at ({cx},{cz})").into_bytes(),
                        compressed_size: 0,
                    };
                    writer.write_chunk(&chunk).unwrap();
                }
            }
        }

        let io_engine = AnvilIoEngine::new_with_engine(IoEngineType::ThreadedFallback);
        let cache = AnvilChunkCache::new(10 * 1024 * 1024, 1024);

        // Prefetch radius around (1, 1) with radius 2
        let summary = cache
            .prefetch_radius("world", &region_dir, 1, 1, 2, &io_engine)
            .unwrap();
        assert!(summary.chunks_loaded > 0);
        assert_eq!(summary.load_failures, 0);

        // Verify chunk in cache
        let cached = cache.get("world", 1, 1).expect("chunk should be cached");
        assert_eq!(cached.payload, b"Payload at (1,1)");

        let stats = cache.stats();
        assert_eq!(stats.hit_count, 1);
        assert!(stats.cached_chunks > 0);

        let (ops, r_bytes, _, savings, batches, _) = io_engine.stats();
        assert!(ops > 0);
        assert!(r_bytes > 0);
        assert!(batches > 0);
        assert!(savings > 0);
    }
}
