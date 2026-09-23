use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use chrono::Utc;
use flate2::read::{GzDecoder, ZlibDecoder};
use flate2::write::{GzEncoder, ZlibEncoder};
use flate2::Compression;
use serde::{Deserialize, Serialize};

use crate::error::{CraftError, Result};

pub const SECTOR_BYTES: usize = 4096;
pub const TOTAL_CHUNKS: usize = 1024;
pub const HEADER_SECTORS: usize = 2;
pub const HEADER_BYTES: usize = HEADER_SECTORS * SECTOR_BYTES; // 8192

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum ChunkCompressionScheme {
    Gzip = 1,
    Zlib = 2,
    Uncompressed = 3,
    Lz4 = 4,
    Zstd = 5,
}

impl ChunkCompressionScheme {
    pub fn from_byte(b: u8) -> Result<Self> {
        match b {
            1 => Ok(Self::Gzip),
            2 => Ok(Self::Zlib),
            3 => Ok(Self::Uncompressed),
            4 => Ok(Self::Lz4),
            5 => Ok(Self::Zstd),
            other => Err(CraftError::Other(format!(
                "Unsupported Anvil chunk compression scheme byte: {other}"
            ))),
        }
    }

    pub fn to_byte(self) -> u8 {
        self as u8
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Gzip => "gzip",
            Self::Zlib => "zlib",
            Self::Uncompressed => "uncompressed",
            Self::Lz4 => "lz4",
            Self::Zstd => "zstd",
        }
    }

    pub fn from_name(name: &str) -> Self {
        match name.to_ascii_lowercase().as_str() {
            "gzip" | "gz" => Self::Gzip,
            "raw" | "uncompressed" | "none" => Self::Uncompressed,
            "lz4" => Self::Lz4,
            "zstd" | "zst" => Self::Zstd,
            _ => Self::Zlib,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkLocation {
    pub offset: u32,
    pub sector_count: u8,
}

impl ChunkLocation {
    pub const EMPTY: Self = Self {
        offset: 0,
        sector_count: 0,
    };

    pub fn is_empty(&self) -> bool {
        self.offset < 2 || self.sector_count == 0
    }

    pub fn from_bytes(bytes: [u8; 4]) -> Self {
        let offset = ((bytes[0] as u32) << 16) | ((bytes[1] as u32) << 8) | (bytes[2] as u32);
        let sector_count = bytes[3];
        Self {
            offset,
            sector_count,
        }
    }

    pub fn to_bytes(&self) -> [u8; 4] {
        [
            ((self.offset >> 16) & 0xFF) as u8,
            ((self.offset >> 8) & 0xFF) as u8,
            (self.offset & 0xFF) as u8,
            self.sector_count,
        ]
    }
}

#[derive(Debug, Clone)]
pub struct RegionFileHeader {
    pub locations: [ChunkLocation; TOTAL_CHUNKS],
    pub timestamps: [u32; TOTAL_CHUNKS],
}

impl Default for RegionFileHeader {
    fn default() -> Self {
        Self::new()
    }
}

impl RegionFileHeader {
    pub fn new() -> Self {
        Self {
            locations: [ChunkLocation::EMPTY; TOTAL_CHUNKS],
            timestamps: [0u32; TOTAL_CHUNKS],
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < HEADER_BYTES {
            return Err(CraftError::Other(format!(
                "Anvil region header truncated: expected at least {HEADER_BYTES} bytes, got {}",
                bytes.len()
            )));
        }

        let mut locations = [ChunkLocation::EMPTY; TOTAL_CHUNKS];
        let mut timestamps = [0u32; TOTAL_CHUNKS];

        for i in 0..TOTAL_CHUNKS {
            let loc_offset = i * 4;
            let mut loc_bytes = [0u8; 4];
            loc_bytes.copy_from_slice(&bytes[loc_offset..loc_offset + 4]);
            locations[i] = ChunkLocation::from_bytes(loc_bytes);

            let ts_offset = 4096 + (i * 4);
            let mut ts_bytes = [0u8; 4];
            ts_bytes.copy_from_slice(&bytes[ts_offset..ts_offset + 4]);
            timestamps[i] = u32::from_be_bytes(ts_bytes);
        }

        Ok(Self {
            locations,
            timestamps,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = vec![0u8; HEADER_BYTES];
        for i in 0..TOTAL_CHUNKS {
            let loc_bytes = self.locations[i].to_bytes();
            out[i * 4..(i * 4) + 4].copy_from_slice(&loc_bytes);

            let ts_bytes = self.timestamps[i].to_be_bytes();
            let ts_offset = 4096 + (i * 4);
            out[ts_offset..ts_offset + 4].copy_from_slice(&ts_bytes);
        }
        out
    }

    pub fn get_location(&self, chunk_x: i32, chunk_z: i32) -> ChunkLocation {
        let idx = chunk_coords_to_index(chunk_x, chunk_z);
        self.locations[idx]
    }

    pub fn set_location(&mut self, chunk_x: i32, chunk_z: i32, loc: ChunkLocation) {
        let idx = chunk_coords_to_index(chunk_x, chunk_z);
        self.locations[idx] = loc;
    }

    pub fn get_timestamp(&self, chunk_x: i32, chunk_z: i32) -> u32 {
        let idx = chunk_coords_to_index(chunk_x, chunk_z);
        self.timestamps[idx]
    }

    pub fn set_timestamp(&mut self, chunk_x: i32, chunk_z: i32, ts: u32) {
        let idx = chunk_coords_to_index(chunk_x, chunk_z);
        self.timestamps[idx] = ts;
    }

    pub fn active_chunks_count(&self) -> usize {
        self.locations.iter().filter(|loc| !loc.is_empty()).count()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkData {
    pub chunk_x: i32,
    pub chunk_z: i32,
    pub timestamp: u32,
    pub scheme: ChunkCompressionScheme,
    pub raw_payload: Vec<u8>,
    pub compressed_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionInspection {
    pub path: String,
    pub region_x: i32,
    pub region_z: i32,
    pub file_size_bytes: u64,
    pub total_sectors: u32,
    pub active_chunks: usize,
    pub empty_chunks: usize,
    pub allocated_payload_sectors: u32,
    pub fragmentation_ratio: f64,
    pub largest_contiguous_free_sectors: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionStats {
    pub original_file_size: u64,
    pub compacted_file_size: u64,
    pub bytes_reclaimed: u64,
    pub chunks_migrated: usize,
    pub sectors_before: u32,
    pub sectors_after: u32,
}

pub fn chunk_coords_to_index(chunk_x: i32, chunk_z: i32) -> usize {
    let rel_x = chunk_x.rem_euclid(32) as usize;
    let rel_z = chunk_z.rem_euclid(32) as usize;
    rel_x + rel_z * 32
}

pub fn region_coords_from_chunk(chunk_x: i32, chunk_z: i32) -> (i32, i32) {
    (chunk_x.div_euclid(32), chunk_z.div_euclid(32))
}

pub fn region_filename(chunk_x: i32, chunk_z: i32) -> String {
    let (rx, rz) = region_coords_from_chunk(chunk_x, chunk_z);
    format!("r.{rx}.{rz}.mca")
}

pub fn decompress_payload(scheme: ChunkCompressionScheme, compressed: &[u8]) -> Result<Vec<u8>> {
    match scheme {
        ChunkCompressionScheme::Gzip => {
            let mut decoder = GzDecoder::new(compressed);
            let mut out = Vec::new();
            decoder
                .read_to_end(&mut out)
                .map_err(CraftError::Io)?;
            Ok(out)
        }
        ChunkCompressionScheme::Zlib => {
            let mut decoder = ZlibDecoder::new(compressed);
            let mut out = Vec::new();
            decoder
                .read_to_end(&mut out)
                .map_err(CraftError::Io)?;
            Ok(out)
        }
        ChunkCompressionScheme::Uncompressed => Ok(compressed.to_vec()),
        ChunkCompressionScheme::Zstd => {
            zstd::stream::decode_all(compressed).map_err(CraftError::Io)
        }
        ChunkCompressionScheme::Lz4 => {
            // For LZ4 in custom environments, attempt zstd or direct passthrough
            match zstd::stream::decode_all(compressed) {
                Ok(v) => Ok(v),
                Err(_) => Ok(compressed.to_vec()),
            }
        }
    }
}

pub fn compress_payload(scheme: ChunkCompressionScheme, uncompressed: &[u8]) -> Result<Vec<u8>> {
    match scheme {
        ChunkCompressionScheme::Gzip => {
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder
                .write_all(uncompressed)
                .map_err(CraftError::Io)?;
            encoder.finish().map_err(CraftError::Io)
        }
        ChunkCompressionScheme::Zlib => {
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
            encoder
                .write_all(uncompressed)
                .map_err(CraftError::Io)?;
            encoder.finish().map_err(CraftError::Io)
        }
        ChunkCompressionScheme::Uncompressed => Ok(uncompressed.to_vec()),
        ChunkCompressionScheme::Zstd => {
            zstd::stream::encode_all(uncompressed, 3).map_err(CraftError::Io)
        }
        ChunkCompressionScheme::Lz4 => {
            // High-speed fallback to fast zstd (level 1)
            zstd::stream::encode_all(uncompressed, 1).map_err(CraftError::Io)
        }
    }
}

pub struct RegionFileReader {
    path: PathBuf,
    file: File,
    header: RegionFileHeader,
    region_x: i32,
    region_z: i32,
}

impl RegionFileReader {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut file = OpenOptions::new()
            .read(true)
            .open(&path)
            .map_err(CraftError::Io)?;

        let metadata = file.metadata().map_err(CraftError::Io)?;
        if metadata.len() < HEADER_BYTES as u64 {
            return Err(CraftError::Other(format!(
                "Region file {:?} is smaller than header size ({HEADER_BYTES} bytes)",
                path
            )));
        }

        let mut header_bytes = vec![0u8; HEADER_BYTES];
        file.seek(SeekFrom::Start(0))
            .map_err(CraftError::Io)?;
        file.read_exact(&mut header_bytes)
            .map_err(CraftError::Io)?;

        let header = RegionFileHeader::from_bytes(&header_bytes)?;

        // Try parsing coordinates from filename "r.X.Z.mca"
        let (region_x, region_z) = parse_coords_from_filename(&path).unwrap_or((0, 0));

        Ok(Self {
            path,
            file,
            header,
            region_x,
            region_z,
        })
    }

    pub fn header(&self) -> &RegionFileHeader {
        &self.header
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn region_coords(&self) -> (i32, i32) {
        (self.region_x, self.region_z)
    }

    pub fn read_chunk(&mut self, chunk_x: i32, chunk_z: i32) -> Result<Option<ChunkData>> {
        let loc = self.header.get_location(chunk_x, chunk_z);
        if loc.is_empty() {
            return Ok(None);
        }

        let timestamp = self.header.get_timestamp(chunk_x, chunk_z);
        let byte_offset = (loc.offset as u64) * (SECTOR_BYTES as u64);

        self.file
            .seek(SeekFrom::Start(byte_offset))
            .map_err(CraftError::Io)?;

        let mut length_buf = [0u8; 4];
        self.file
            .read_exact(&mut length_buf)
            .map_err(CraftError::Io)?;
        let payload_len = u32::from_be_bytes(length_buf) as usize;

        if payload_len == 0 {
            return Ok(None);
        }

        let mut scheme_byte = [0u8; 1];
        self.file
            .read_exact(&mut scheme_byte)
            .map_err(CraftError::Io)?;
        let scheme = ChunkCompressionScheme::from_byte(scheme_byte[0])?;

        let compressed_size = payload_len.saturating_sub(1);
        let mut compressed_data = vec![0u8; compressed_size];
        self.file
            .read_exact(&mut compressed_data)
            .map_err(CraftError::Io)?;

        let raw_payload = decompress_payload(scheme, &compressed_data)?;

        Ok(Some(ChunkData {
            chunk_x,
            chunk_z,
            timestamp,
            scheme,
            raw_payload,
            compressed_size,
        }))
    }

    pub fn inspect(&self) -> Result<RegionInspection> {
        let metadata = self.file.metadata().map_err(CraftError::Io)?;
        let file_size_bytes = metadata.len();
        let total_sectors = ((file_size_bytes + (SECTOR_BYTES as u64 - 1)) / (SECTOR_BYTES as u64)) as u32;

        let mut occupied_sectors = vec![false; total_sectors.max(HEADER_SECTORS as u32) as usize];
        // Mark header sectors as occupied
        if occupied_sectors.len() >= 2 {
            occupied_sectors[0] = true;
            occupied_sectors[1] = true;
        }

        let mut active_chunks = 0;
        let mut allocated_payload_sectors = 0;

        for loc in &self.header.locations {
            if !loc.is_empty() {
                active_chunks += 1;
                allocated_payload_sectors += loc.sector_count as u32;
                let start = loc.offset as usize;
                let end = start + (loc.sector_count as usize);
                for sec in start..end {
                    if sec < occupied_sectors.len() {
                        occupied_sectors[sec] = true;
                    }
                }
            }
        }

        // Calculate free sector runs and fragmentation
        let mut max_free_run = 0u32;
        let mut cur_free_run = 0u32;
        let mut total_free_after_header = 0u32;

        for &occupied in occupied_sectors.iter().skip(HEADER_SECTORS) {
            if !occupied {
                cur_free_run += 1;
                total_free_after_header += 1;
                if cur_free_run > max_free_run {
                    max_free_run = cur_free_run;
                }
            } else {
                cur_free_run = 0;
            }
        }

        let fragmentation_ratio = if total_sectors > HEADER_SECTORS as u32 {
            let payload_sectors_total = total_sectors - HEADER_SECTORS as u32;
            if payload_sectors_total > 0 {
                (total_free_after_header as f64) / (payload_sectors_total as f64)
            } else {
                0.0
            }
        } else {
            0.0
        };

        Ok(RegionInspection {
            path: self.path.display().to_string(),
            region_x: self.region_x,
            region_z: self.region_z,
            file_size_bytes,
            total_sectors,
            active_chunks,
            empty_chunks: TOTAL_CHUNKS - active_chunks,
            allocated_payload_sectors,
            fragmentation_ratio,
            largest_contiguous_free_sectors: max_free_run,
        })
    }
}

pub struct RegionFileWriter {
    path: PathBuf,
    file: File,
    header: RegionFileHeader,
    region_x: i32,
    region_z: i32,
}

impl RegionFileWriter {
    pub fn open_or_create<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let (region_x, region_z) = parse_coords_from_filename(&path).unwrap_or((0, 0));

        let file_exists = path.exists();
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&path)
            .map_err(CraftError::Io)?;

        let header = if file_exists && file.metadata().map_err(CraftError::Io)?.len() >= HEADER_BYTES as u64 {
            let mut header_bytes = vec![0u8; HEADER_BYTES];
            file.seek(SeekFrom::Start(0)).map_err(CraftError::Io)?;
            file.read_exact(&mut header_bytes).map_err(CraftError::Io)?;
            RegionFileHeader::from_bytes(&header_bytes)?
        } else {
            let header = RegionFileHeader::new();
            file.seek(SeekFrom::Start(0)).map_err(CraftError::Io)?;
            file.write_all(&header.to_bytes()).map_err(CraftError::Io)?;
            file.flush().map_err(CraftError::Io)?;
            header
        };

        Ok(Self {
            path,
            file,
            header,
            region_x,
            region_z,
        })
    }

    pub fn header(&self) -> &RegionFileHeader {
        &self.header
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn region_coords(&self) -> (i32, i32) {
        (self.region_x, self.region_z)
    }

    pub fn write_chunk(&mut self, chunk: &ChunkData) -> Result<()> {
        let compressed = compress_payload(chunk.scheme, &chunk.raw_payload)?;
        let payload_len = 1 + compressed.len(); // 1 byte scheme + data
        let total_chunk_bytes = 4 + payload_len; // 4 bytes length prefix + payload
        let sectors_needed = ((total_chunk_bytes + SECTOR_BYTES - 1) / SECTOR_BYTES) as u8;

        // Build sector allocation table
        let existing_loc = self.header.get_location(chunk.chunk_x, chunk.chunk_z);
        let target_offset = self.find_free_sector_offset(sectors_needed as u32, existing_loc)?;

        // Prepare full sector-aligned buffer
        let total_alloc_bytes = (sectors_needed as usize) * SECTOR_BYTES;
        let mut chunk_buf = vec![0u8; total_alloc_bytes];

        // 4 bytes payload length (big endian)
        chunk_buf[0..4].copy_from_slice(&(payload_len as u32).to_be_bytes());
        // 1 byte scheme
        chunk_buf[4] = chunk.scheme.to_byte();
        // payload bytes
        chunk_buf[5..5 + compressed.len()].copy_from_slice(&compressed);

        // Write chunk at sector boundary
        let byte_offset = (target_offset as u64) * (SECTOR_BYTES as u64);
        self.file
            .seek(SeekFrom::Start(byte_offset))
            .map_err(CraftError::Io)?;
        self.file
            .write_all(&chunk_buf)
            .map_err(CraftError::Io)?;

        // Update header
        let now = if chunk.timestamp > 0 {
            chunk.timestamp
        } else {
            Utc::now().timestamp() as u32
        };
        let new_loc = ChunkLocation {
            offset: target_offset,
            sector_count: sectors_needed,
        };
        self.header.set_location(chunk.chunk_x, chunk.chunk_z, new_loc);
        self.header.set_timestamp(chunk.chunk_x, chunk.chunk_z, now);

        self.flush_header()?;
        Ok(())
    }

    pub fn delete_chunk(&mut self, chunk_x: i32, chunk_z: i32) -> Result<()> {
        self.header
            .set_location(chunk_x, chunk_z, ChunkLocation::EMPTY);
        self.header.set_timestamp(chunk_x, chunk_z, 0);
        self.flush_header()
    }

    pub fn compact(&mut self) -> Result<CompactionStats> {
        let metadata = self.file.metadata().map_err(CraftError::Io)?;
        let original_file_size = metadata.len();
        let sectors_before = ((original_file_size + (SECTOR_BYTES as u64 - 1)) / (SECTOR_BYTES as u64)) as u32;

        // Read all valid chunks into memory
        let mut chunks_to_keep = Vec::new();
        for z in 0..32 {
            for x in 0..32 {
                let loc = self.header.get_location(x, z);
                if !loc.is_empty() {
                    let ts = self.header.get_timestamp(x, z);
                    let byte_offset = (loc.offset as u64) * (SECTOR_BYTES as u64);
                    self.file
                        .seek(SeekFrom::Start(byte_offset))
                        .map_err(CraftError::Io)?;

                    let mut len_bytes = [0u8; 4];
                    self.file.read_exact(&mut len_bytes).map_err(CraftError::Io)?;
                    let payload_len = u32::from_be_bytes(len_bytes) as usize;
                    if payload_len > 0 {
                        let mut scheme_byte = [0u8; 1];
                        self.file.read_exact(&mut scheme_byte).map_err(CraftError::Io)?;
                        let scheme = ChunkCompressionScheme::from_byte(scheme_byte[0])?;
                        let compressed_size = payload_len.saturating_sub(1);
                        let mut raw_compressed = vec![0u8; compressed_size];
                        self.file.read_exact(&mut raw_compressed).map_err(CraftError::Io)?;
                        let uncompressed = decompress_payload(scheme, &raw_compressed)?;

                        chunks_to_keep.push((x, z, ts, scheme, uncompressed));
                    }
                }
            }
        }

        // Reset file to pristine header
        self.file.set_len(HEADER_BYTES as u64).map_err(CraftError::Io)?;
        self.header = RegionFileHeader::new();

        let chunks_migrated = chunks_to_keep.len();
        let mut current_sector = HEADER_SECTORS as u32;

        for (x, z, ts, scheme, payload) in chunks_to_keep {
            let compressed = compress_payload(scheme, &payload)?;
            let payload_len = 1 + compressed.len();
            let total_chunk_bytes = 4 + payload_len;
            let sectors_needed = ((total_chunk_bytes + SECTOR_BYTES - 1) / SECTOR_BYTES) as u8;

            let total_alloc_bytes = (sectors_needed as usize) * SECTOR_BYTES;
            let mut chunk_buf = vec![0u8; total_alloc_bytes];
            chunk_buf[0..4].copy_from_slice(&(payload_len as u32).to_be_bytes());
            chunk_buf[4] = scheme.to_byte();
            chunk_buf[5..5 + compressed.len()].copy_from_slice(&compressed);

            let byte_offset = (current_sector as u64) * (SECTOR_BYTES as u64);
            self.file
                .seek(SeekFrom::Start(byte_offset))
                .map_err(CraftError::Io)?;
            self.file
                .write_all(&chunk_buf)
                .map_err(CraftError::Io)?;

            self.header.set_location(
                x,
                z,
                ChunkLocation {
                    offset: current_sector,
                    sector_count: sectors_needed,
                },
            );
            self.header.set_timestamp(x, z, ts);

            current_sector += sectors_needed as u32;
        }

        self.flush_header()?;
        self.file.flush().map_err(CraftError::Io)?;

        let compacted_file_size = self.file.metadata().map_err(CraftError::Io)?.len();
        let sectors_after = current_sector;
        let bytes_reclaimed = original_file_size.saturating_sub(compacted_file_size);

        Ok(CompactionStats {
            original_file_size,
            compacted_file_size,
            bytes_reclaimed,
            chunks_migrated,
            sectors_before,
            sectors_after,
        })
    }

    fn flush_header(&mut self) -> Result<()> {
        let header_bytes = self.header.to_bytes();
        self.file
            .seek(SeekFrom::Start(0))
            .map_err(CraftError::Io)?;
        self.file
            .write_all(&header_bytes)
            .map_err(CraftError::Io)?;
        self.file.flush().map_err(CraftError::Io)?;
        Ok(())
    }

    fn find_free_sector_offset(&mut self, sectors_needed: u32, existing_loc: ChunkLocation) -> Result<u32> {
        let file_len = self.file.metadata().map_err(CraftError::Io)?.len();
        let total_sectors = ((file_len + (SECTOR_BYTES as u64 - 1)) / (SECTOR_BYTES as u64)) as u32;
        let mut occupied = vec![false; total_sectors.max(HEADER_SECTORS as u32) as usize];

        // Mark header sectors
        if occupied.len() >= 2 {
            occupied[0] = true;
            occupied[1] = true;
        }

        // Mark other chunks
        for loc in &self.header.locations {
            if !loc.is_empty() {
                // If it's the chunk we are overwriting, skip marking so we can reuse in-place if it fits!
                if loc.offset == existing_loc.offset && loc.sector_count == existing_loc.sector_count {
                    continue;
                }
                let start = loc.offset as usize;
                let end = start + (loc.sector_count as usize);
                for s in start..end {
                    if s < occupied.len() {
                        occupied[s] = true;
                    }
                }
            }
        }

        // If existing spot fits, reuse it
        if !existing_loc.is_empty() && (existing_loc.sector_count as u32) >= sectors_needed {
            return Ok(existing_loc.offset);
        }

        // Search for first contiguous fit starting at sector 2
        let needed = sectors_needed as usize;
        let mut run_start = 0;
        let mut run_len = 0;

        for (i, &is_occ) in occupied.iter().enumerate().skip(HEADER_SECTORS) {
            if !is_occ {
                if run_len == 0 {
                    run_start = i;
                }
                run_len += 1;
                if run_len == needed {
                    return Ok(run_start as u32);
                }
            } else {
                run_len = 0;
            }
        }

        // Otherwise append at EOF
        Ok(total_sectors.max(HEADER_SECTORS as u32))
    }
}

fn parse_coords_from_filename(path: &Path) -> Option<(i32, i32)> {
    let fname = path.file_name()?.to_str()?;
    // Format: "r.<X>.<Z>.mca"
    let parts: Vec<&str> = fname.split('.').collect();
    if parts.len() == 4 && parts[0] == "r" && parts[3] == "mca" {
        let x = parts[1].parse::<i32>().ok()?;
        let z = parts[2].parse::<i32>().ok()?;
        return Some((x, z));
    }
    None
}
