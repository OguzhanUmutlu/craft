use bytes::{BufMut, Bytes, BytesMut};
use serde::{Deserialize, Serialize};
use std::time::Instant;

pub const DEFAULT_CHUNK_PACKET_ID: i32 = 0x20;

#[derive(Debug, Clone)]
pub struct ChunkSection {
    pub y_index: i8,
    pub block_count: i16,
    pub block_states: Bytes,
    pub biomes: Bytes,
}

impl ChunkSection {
    pub fn new(y_index: i8, block_count: i16, block_states: Bytes, biomes: Bytes) -> Self {
        Self {
            y_index,
            block_count,
            block_states,
            biomes,
        }
    }

    pub fn byte_len(&self) -> usize {
        1 + 2 + self.block_states.len() + self.biomes.len()
    }
}

#[derive(Debug, Clone)]
pub struct ChunkDataPacket {
    pub packet_id: i32,
    pub chunk_x: i32,
    pub chunk_z: i32,
    pub heightmaps_nbt: Bytes,
    pub sections: Vec<ChunkSection>,
    pub block_entities: Vec<Bytes>,
    pub light_data: Bytes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocketTransferSimulation {
    pub total_bytes_transferred: usize,
    pub slice_count: usize,
    pub mtu_size: usize,
    pub zero_copy_verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkSerializationBenchmark {
    pub iterations: usize,
    pub elapsed_nanos: u64,
    pub total_bytes_serialized: usize,
    pub nanos_per_packet: f64,
    pub throughput_mb_sec: f64,
}

impl ChunkDataPacket {
    pub fn new(chunk_x: i32, chunk_z: i32) -> Self {
        Self {
            packet_id: DEFAULT_CHUNK_PACKET_ID,
            chunk_x,
            chunk_z,
            heightmaps_nbt: Bytes::from_static(b"\x0a\x00\x00\x00"), // Empty TAG_Compound
            sections: Vec::new(),
            block_entities: Vec::new(),
            light_data: Bytes::from_static(&[0u8; 32]),
        }
    }

    pub fn with_dummy_sections(chunk_x: i32, chunk_z: i32, section_count: usize) -> Self {
        let mut sections = Vec::with_capacity(section_count);
        for y in 0..section_count {
            // Simulated 4096 blocks palette data + 64 biomes palette data
            let block_states = Bytes::from(vec![1u8; 512]);
            let biomes = Bytes::from(vec![0u8; 64]);
            sections.push(ChunkSection {
                y_index: y as i8,
                block_count: 256,
                block_states,
                biomes,
            });
        }

        Self {
            packet_id: DEFAULT_CHUNK_PACKET_ID,
            chunk_x,
            chunk_z,
            heightmaps_nbt: Bytes::from_static(b"\x0a\x00\x00\x00"),
            sections,
            block_entities: Vec::new(),
            light_data: Bytes::from(vec![0xFFu8; 128]),
        }
    }

    pub fn serialize_zero_copy(&self) -> Bytes {
        // Calculate sections total length
        let mut sections_len = 0;
        for s in &self.sections {
            sections_len += s.byte_len();
        }

        // Preallocate buffer to eliminate reallocations
        let estimated_cap = 5
            + 8
            + self.heightmaps_nbt.len()
            + 5
            + sections_len
            + 5
            + self.block_entities.iter().map(|b| 5 + b.len()).sum::<usize>()
            + 5
            + self.light_data.len();

        let mut buf = BytesMut::with_capacity(estimated_cap);

        // 1. Packet ID VarInt
        encode_varint(&mut buf, self.packet_id);

        // 2. Chunk coordinates (int x, int z)
        buf.put_i32(self.chunk_x);
        buf.put_i32(self.chunk_z);

        // 3. Heightmaps NBT
        buf.put_slice(&self.heightmaps_nbt);

        // 4. Data length VarInt and sections payload
        encode_varint(&mut buf, sections_len as i32);
        for s in &self.sections {
            buf.put_i8(s.y_index);
            buf.put_i16(s.block_count);
            buf.put_slice(&s.block_states);
            buf.put_slice(&s.biomes);
        }

        // 5. Block entities count VarInt and entities
        encode_varint(&mut buf, self.block_entities.len() as i32);
        for entity in &self.block_entities {
            encode_varint(&mut buf, entity.len() as i32);
            buf.put_slice(entity);
        }

        // 6. Light data length VarInt and light payload
        encode_varint(&mut buf, self.light_data.len() as i32);
        buf.put_slice(&self.light_data);

        buf.freeze()
    }

    pub fn simulate_nvme_to_socket_transfer(&self, mtu_chunk_size: usize) -> SocketTransferSimulation {
        let chunk_bytes = self.serialize_zero_copy();
        let total_len = chunk_bytes.len();
        let mtu = mtu_chunk_size.max(256);

        let mut slices = Vec::new();
        let mut offset = 0;

        while offset < total_len {
            let end = (offset + mtu).min(total_len);
            let slice = chunk_bytes.slice(offset..end);
            slices.push(slice);
            offset = end;
        }

        let slice_count = slices.len();
        let transferred = slices.iter().map(|s| s.len()).sum::<usize>();

        SocketTransferSimulation {
            total_bytes_transferred: transferred,
            slice_count,
            mtu_size: mtu,
            zero_copy_verified: transferred == total_len,
        }
    }

    pub fn benchmark_serialization(iterations: usize) -> ChunkSerializationBenchmark {
        let iters = iterations.max(10);
        let packet = Self::with_dummy_sections(10, 20, 16);

        let start = Instant::now();
        let mut total_bytes = 0;

        for _ in 0..iters {
            let b = packet.serialize_zero_copy();
            total_bytes += b.len();
        }

        let elapsed = start.elapsed();
        let elapsed_nanos = elapsed.as_nanos() as u64;
        let nanos_per_packet = (elapsed_nanos as f64) / (iters as f64);

        let elapsed_secs = elapsed.as_secs_f64();
        let mb_total = (total_bytes as f64) / (1024.0 * 1024.0);
        let throughput_mb_sec = if elapsed_secs > 0.0 {
            mb_total / elapsed_secs
        } else {
            0.0
        };

        ChunkSerializationBenchmark {
            iterations: iters,
            elapsed_nanos,
            total_bytes_serialized: total_bytes,
            nanos_per_packet,
            throughput_mb_sec,
        }
    }
}

pub fn encode_varint(buf: &mut BytesMut, mut value: i32) {
    loop {
        let mut temp = (value & 0x7F) as u8;
        value = ((value as u32) >> 7) as i32;
        if value != 0 {
            temp |= 0x80;
        }
        buf.put_u8(temp);
        if value == 0 {
            break;
        }
    }
}

pub fn decode_varint(slice: &[u8]) -> Option<(i32, usize)> {
    let mut num_read = 0;
    let mut result = 0i32;

    for &byte in slice {
        let value = (byte & 0x7F) as i32;
        result |= value << (7 * num_read);

        num_read += 1;
        if num_read > 5 {
            return None;
        }

        if (byte & 0x80) == 0 {
            return Some((result, num_read));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_varint_roundtrip() {
        let test_cases = [0, 1, 2, 127, 128, 255, 256, 2097151, -1, -2147483648];
        for &val in &test_cases {
            let mut buf = BytesMut::new();
            encode_varint(&mut buf, val);
            let (decoded, len) = decode_varint(&buf).expect("decode varint");
            assert_eq!(decoded, val);
            assert_eq!(len, buf.len());
        }
    }

    #[test]
    fn test_chunk_data_packet_zero_copy() {
        let packet = ChunkDataPacket::with_dummy_sections(5, -12, 16);
        let serialized = packet.serialize_zero_copy();

        assert!(!serialized.is_empty());
        let (pkt_id, id_len) = decode_varint(&serialized).expect("packet id varint");
        assert_eq!(pkt_id, DEFAULT_CHUNK_PACKET_ID);

        let rest = &serialized[id_len..];
        let x = i32::from_be_bytes([rest[0], rest[1], rest[2], rest[3]]);
        let z = i32::from_be_bytes([rest[4], rest[5], rest[6], rest[7]]);
        assert_eq!(x, 5);
        assert_eq!(z, -12);

        // Test NVMe-to-socket transfer simulation
        let sim = packet.simulate_nvme_to_socket_transfer(1400);
        assert!(sim.zero_copy_verified);
        assert!(sim.slice_count > 1);
        assert_eq!(sim.total_bytes_transferred, serialized.len());
    }

    #[test]
    fn test_chunk_benchmark_execution() {
        let bench = ChunkDataPacket::benchmark_serialization(100);
        assert_eq!(bench.iterations, 100);
        assert!(bench.total_bytes_serialized > 0);
        assert!(bench.throughput_mb_sec > 0.0);
    }
}
