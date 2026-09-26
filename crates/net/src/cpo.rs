// crates/net/src/cpo.rs
//
// Phase 53: Autonomous Silicon Photonic Co-Packaged Optics (CPO), Optical Neural Matrix Multiply & Sub-Nanosecond Direct Die Interconnects

use std::time::Instant;
use craft_core::cpo::{
    CpoBenchmarkMetrics, CpoThermalServoState, CpoThermalStatus, MziMesh,
};
use craft_core::error::{CraftError, Result};
use craft_core::compute_crc32;

pub const CPO_FRAME_MAGIC: &[u8; 4] = b"CPO1"; // 0x43504F31

pub const FLAG_ANALOG_MVM: u16 = 0x0001;
pub const FLAG_THERMAL_STABILIZED: u16 = 0x0002;
pub const FLAG_LOOPBACK: u16 = 0x0004;

/// High-speed binary framing for Co-Packaged Optics tensor input/output streaming
#[derive(Debug, Clone, PartialEq)]
pub struct CpoTensorFrame {
    pub tile_id: u32,
    pub sequence_number: u64,
    pub flags: u16,
    pub vector_dim: u16,
    pub checksum: u32,
    pub payload: Vec<f32>,
}

impl CpoTensorFrame {
    pub fn new(tile_id: u32, sequence_number: u64, flags: u16, payload: Vec<f32>) -> Self {
        let vector_dim = payload.len() as u16;
        let mut raw_bytes = Vec::with_capacity(payload.len() * 4);
        for val in &payload {
            raw_bytes.extend_from_slice(&val.to_be_bytes());
        }
        let checksum = compute_crc32(&raw_bytes);

        Self {
            tile_id,
            sequence_number,
            flags,
            vector_dim,
            checksum,
            payload,
        }
    }

    /// Serializes frame to binary wire bytes (24 bytes header + payload)
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(24 + (self.payload.len() * 4));
        buf.extend_from_slice(CPO_FRAME_MAGIC);
        buf.extend_from_slice(&self.tile_id.to_be_bytes());
        buf.extend_from_slice(&self.sequence_number.to_be_bytes());
        buf.extend_from_slice(&self.flags.to_be_bytes());
        buf.extend_from_slice(&self.vector_dim.to_be_bytes());
        buf.extend_from_slice(&self.checksum.to_be_bytes());

        for val in &self.payload {
            buf.extend_from_slice(&val.to_be_bytes());
        }
        buf
    }

    /// Deserializes frame from binary wire bytes
    pub fn decode(buf: &[u8]) -> Result<Self> {
        if buf.len() < 24 {
            return Err(CraftError::Config(format!(
                "CPO frame buffer too small: {} bytes (expected at least 24)",
                buf.len()
            )));
        }

        if &buf[0..4] != CPO_FRAME_MAGIC {
            return Err(CraftError::Config(format!(
                "Invalid CPO frame magic: {:02X?}",
                &buf[0..4]
            )));
        }

        let tile_id = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
        let sequence_number = u64::from_be_bytes([
            buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
        ]);
        let flags = u16::from_be_bytes([buf[16], buf[17]]);
        let vector_dim = u16::from_be_bytes([buf[18], buf[19]]) as usize;
        let checksum = u32::from_be_bytes([buf[20], buf[21], buf[22], buf[23]]);

        let expected_payload_bytes = vector_dim * 4;
        if buf.len() < 24 + expected_payload_bytes {
            return Err(CraftError::Config(format!(
                "Incomplete CPO frame: expected {} bytes of payload, got {}",
                expected_payload_bytes,
                buf.len() - 24
            )));
        }

        let payload_slice = &buf[24..24 + expected_payload_bytes];
        let computed_crc = compute_crc32(payload_slice);
        if computed_crc != checksum {
            return Err(CraftError::Config(format!(
                "CPO frame CRC32 mismatch: expected {:08X}, got {:08X}",
                checksum, computed_crc
            )));
        }

        let mut payload = Vec::with_capacity(vector_dim);
        for chunk in payload_slice.chunks_exact(4) {
            let bytes = [chunk[0], chunk[1], chunk[2], chunk[3]];
            payload.push(f32::from_be_bytes(bytes));
        }

        Ok(Self {
            tile_id,
            sequence_number,
            flags,
            vector_dim: vector_dim as u16,
            checksum,
            payload,
        })
    }
}

/// Simulated optical matrix-vector multiply (MVM) computing engine
#[derive(Debug, Clone)]
pub struct PhotonicTensorEngine {
    pub mesh: MziMesh,
    pub energy_efficiency_pj_per_mac: f64,
}

impl PhotonicTensorEngine {
    pub fn new(mesh: MziMesh) -> Self {
        let energy_efficiency_pj_per_mac = mesh.energy_efficiency_pj_per_mac;
        Self {
            mesh,
            energy_efficiency_pj_per_mac,
        }
    }

    /// Perform forward optical MVM pass
    /// Returns: (result_vector, mac_operations, latency_picoseconds, energy_picojoules)
    pub fn forward_vector(&self, input: &[f32]) -> (Vec<f32>, u64, f64, f64) {
        let f64_input: Vec<f64> = input.iter().map(|&v| v as f64).collect();
        let f64_output = self.mesh.multiply_vector(&f64_input);
        let output: Vec<f32> = f64_output.iter().map(|&v| v as f32).collect();

        let mac_operations = (input.len() * self.mesh.rows) as u64;
        let latency_ps = 145.0 + (self.mesh.cols as f64 * 32.5);
        let energy_pj = mac_operations as f64 * self.energy_efficiency_pj_per_mac;

        (output, mac_operations, latency_ps, energy_pj)
    }

    /// Evaluates mean relative L2 error compared to pure electronic floating-point MVM
    pub fn evaluate_accuracy(&self, input: &[f32]) -> f64 {
        let (actual, _, _, _) = self.forward_vector(input);
        let mut expected = vec![0.0f32; self.mesh.rows];
        let in_len = input.len().min(self.mesh.cols);

        for r in 0..self.mesh.rows {
            let mut sum = 0.0f32;
            for c in 0..in_len {
                let cell_idx = r * self.mesh.cols + c;
                if let Some(cell) = self.mesh.cells.get(cell_idx) {
                    sum += (cell.phase_shift_theta.cos() as f32) * input[c];
                }
            }
            expected[r] = sum;
        }

        let mut diff_sq_sum = 0.0;
        let mut norm_sq_sum = 0.0;
        for i in 0..actual.len() {
            let diff = actual[i] - expected[i];
            diff_sq_sum += (diff * diff) as f64;
            norm_sq_sum += (expected[i] * expected[i]) as f64;
        }

        if norm_sq_sum > 1e-12 {
            (diff_sq_sum / norm_sq_sum).sqrt()
        } else {
            0.0
        }
    }
}

/// Closed-loop PID wavelength drift servo and thermal micro-ring regulator
#[derive(Debug, Clone)]
pub struct CpoThermalRegulator {
    pub target_temp_c: f64,
    pub current_temp_c: f64,
    pub kp: f64,
    pub ki: f64,
    pub kd: f64,
    pub integral: f64,
    pub last_error: f64,
}

impl Default for CpoThermalRegulator {
    fn default() -> Self {
        Self::new(45.0)
    }
}

impl CpoThermalRegulator {
    pub fn new(target_temp_c: f64) -> Self {
        Self {
            target_temp_c,
            current_temp_c: target_temp_c + 0.05,
            kp: 1.25,
            ki: 0.08,
            kd: 0.35,
            integral: 0.0,
            last_error: 0.0,
        }
    }

    /// Process a thermal servo loop step
    pub fn step(&mut self, measured_temp_c: f64) -> CpoThermalServoState {
        self.current_temp_c = measured_temp_c;
        let error = self.current_temp_c - self.target_temp_c;
        self.integral = (self.integral + error).clamp(-20.0, 20.0);
        let derivative = error - self.last_error;
        self.last_error = error;

        let correction = self.kp * error + self.ki * self.integral + self.kd * derivative;
        self.current_temp_c -= correction * 0.35;

        let post_error = self.current_temp_c - self.target_temp_c;
        let drift_nm = (post_error * 82.0) / 1000.0;
        let thermal_status = if post_error.abs() < 0.25 {
            CpoThermalStatus::Locked
        } else if post_error.abs() < 1.5 {
            CpoThermalStatus::Tuning
        } else {
            CpoThermalStatus::DriftWarning
        };

        CpoThermalServoState {
            substrate_temp_c: self.current_temp_c,
            target_temp_c: self.target_temp_c,
            drift_nm,
            pid_integral: self.integral,
            pid_derivative: derivative,
            thermal_status,
        }
    }

    /// Inject a thermal shock disturbance (e.g. sudden high-throughput load)
    pub fn inject_thermal_disturbance(&mut self, delta_c: f64) {
        self.current_temp_c += delta_c;
    }
}

/// Run an empirical benchmark sweep over the optical tensor pipeline
pub fn benchmark_cpo_interconnect(iterations: usize, vector_dim: usize) -> CpoBenchmarkMetrics {
    let rows = vector_dim.clamp(2, 32);
    let cols = rows;
    let mesh = MziMesh::new("Benchmark-Mesh-0", rows, cols);
    let engine = PhotonicTensorEngine::new(mesh);

    let test_vector: Vec<f32> = (0..cols).map(|i| (i as f32 * 0.1) + 0.5).collect();

    let start = Instant::now();
    let mut total_mac: u64 = 0;
    let mut total_energy_pj = 0.0;

    for _ in 0..iterations {
        let (_, mac, _, energy_pj) = engine.forward_vector(&test_vector);
        total_mac += mac;
        total_energy_pj += energy_pj;
    }

    let elapsed = start.elapsed();
    let duration_nanos = elapsed.as_nanos().max(1) as u64;
    let duration_secs = elapsed.as_secs_f64().max(1e-9);

    let throughput_tops = (total_mac as f64) / (duration_secs * 1e12);
    let total_joules = total_energy_pj * 1e-12;
    let power_consumption_watts = total_joules / duration_secs;
    let energy_efficiency_pj_per_mac = if total_mac > 0 {
        total_energy_pj / (total_mac as f64)
    } else {
        0.075
    };

    let avg_vector_error_l2 = engine.evaluate_accuracy(&test_vector);

    CpoBenchmarkMetrics {
        mac_operations: total_mac,
        duration_nanos,
        throughput_tops,
        power_consumption_watts,
        energy_efficiency_pj_per_mac,
        avg_vector_error_l2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpo_frame_roundtrip() {
        let payload = vec![1.0f32, 2.5, -3.14, 0.042];
        let frame = CpoTensorFrame::new(1, 42, FLAG_ANALOG_MVM | FLAG_THERMAL_STABILIZED, payload.clone());

        let encoded = frame.encode();
        assert_eq!(encoded.len(), 24 + 16);

        let decoded = CpoTensorFrame::decode(&encoded).unwrap();
        assert_eq!(decoded.tile_id, 1);
        assert_eq!(decoded.sequence_number, 42);
        assert_eq!(decoded.flags, FLAG_ANALOG_MVM | FLAG_THERMAL_STABILIZED);
        assert_eq!(decoded.vector_dim, 4);
        assert_eq!(decoded.payload, payload);
    }

    #[test]
    fn test_cpo_frame_invalid_magic_and_checksum() {
        let mut encoded = CpoTensorFrame::new(0, 1, 0, vec![1.0, 2.0]).encode();
        // Corrupt magic
        encoded[0] = b'X';
        assert!(CpoTensorFrame::decode(&encoded).is_err());

        // Fix magic and corrupt payload
        encoded[0] = b'C';
        let last_idx = encoded.len() - 1;
        encoded[last_idx] ^= 0xFF;
        assert!(CpoTensorFrame::decode(&encoded).is_err());
    }

    #[test]
    fn test_photonic_tensor_engine_mvm() {
        let mesh = MziMesh::new("TestMesh", 4, 4);
        let engine = PhotonicTensorEngine::new(mesh);
        let input = vec![1.0f32, 1.0, 1.0, 1.0];

        let (output, mac, latency_ps, energy_pj) = engine.forward_vector(&input);
        assert_eq!(output.len(), 4);
        assert_eq!(mac, 16);
        assert!(latency_ps > 100.0);
        assert!(energy_pj > 0.0);

        let error = engine.evaluate_accuracy(&input);
        assert!(error < 1e-4);
    }

    #[test]
    fn test_cpo_thermal_regulator_pid_convergence() {
        let mut regulator = CpoThermalRegulator::new(45.0);
        regulator.inject_thermal_disturbance(5.0); // Heat to 50.0 C

        let mut status = regulator.step(regulator.current_temp_c);
        assert_eq!(status.thermal_status, CpoThermalStatus::DriftWarning);

        // Run servo loop for multiple steps to converge
        for _ in 0..50 {
            status = regulator.step(regulator.current_temp_c);
        }

        assert_eq!(status.thermal_status, CpoThermalStatus::Locked);
        assert!((status.substrate_temp_c - 45.0).abs() < 0.25);
    }

    #[test]
    fn test_benchmark_cpo_interconnect() {
        let metrics = benchmark_cpo_interconnect(500, 4);
        assert_eq!(metrics.mac_operations, 500 * 16);
        assert!(metrics.throughput_tops > 0.0);
        assert!(metrics.power_consumption_watts > 0.0);
        assert!(metrics.energy_efficiency_pj_per_mac > 0.0);
        assert!(metrics.avg_vector_error_l2 < 1e-4);
    }
}
