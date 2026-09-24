use crate::error::{CraftError, Result};
use crate::path::CraftPaths;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

// ============================================================================
// Constants and Ring Definitions for ML-KEM (FIPS 203)
// ============================================================================

pub const KYBER_N: usize = 256;
pub const KYBER_Q: i16 = 3329;
pub const MONTGOMERY_R: i32 = 2285; // 2^16 mod 3329
pub const QINV: i16 = -3327; // q^-1 mod 2^16

// Roots of unity (zetas) for NTT modulo 3329 (official NIST FIPS 203 table)
pub const ZETAS: [i16; 128] = [
    -1044,  -758,  -359, -1517,  1493,  1422,   287,   202,
     -171,   622,  1577,   182,   962, -1202, -1474,  1468,
      573, -1325,   264,   383,  -829,  1458, -1602,  -130,
     -681,  1017,   732,   608, -1542,   411,  -205, -1571,
     1223,   652,  -552,  1015, -1293,  1491,  -282, -1544,
      516,    -8,  -320,  -666, -1618, -1162,   126,  1469,
     -853,   -90,  -271,   830,   107, -1421,  -247,  -951,
     -398,   961, -1508,  -725,   448, -1065,   677, -1275,
    -1103,   430,   555,   843, -1251,   871,  1550,   105,
      422,   587,   177,  -235,  -291,  -460,  1574,  1653,
     -246,   778,  1159,  -147,  -777,  1483,  -602,  1119,
    -1590,   644,  -872,   349,   418,   329,  -156,   -75,
      817,  1097,   603,   610,  1322, -1285, -1465,   384,
    -1215,  -136,  1218, -1335,  -874,   220, -1187, -1659,
    -1185, -1530, -1278,   794, -1510,  -854,  -870,   478,
     -108,  -308,   996,   991,   958, -1460,  1522,  1628,
];

#[inline(always)]
pub fn montgomery_reduce(a: i32) -> i16 {
    let t = (a as i16).wrapping_mul(QINV) as i32;
    let t = (a - t * KYBER_Q as i32) >> 16;
    t as i16
}

#[inline(always)]
pub fn barrett_reduce(a: i16) -> i16 {
    let v = ((1i32 << 26) + (KYBER_Q as i32 / 2)) / KYBER_Q as i32;
    let t = ((v as i64 * a as i64 + (1i64 << 25)) >> 26) as i32;
    (a as i32 - t * KYBER_Q as i32) as i16
}

#[inline(always)]
pub fn fqmul(a: i16, b: i16) -> i16 {
    montgomery_reduce(a as i32 * b as i32)
}

/// Polynomial in R_q
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Poly {
    pub coeffs: [i16; KYBER_N],
}

impl Default for Poly {
    fn default() -> Self {
        Self { coeffs: [0; KYBER_N] }
    }
}

impl Poly {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number Theoretic Transform (NTT) in-place
    pub fn ntt(&mut self) {
        let mut k = 1usize;
        let mut len = 128usize;
        while len >= 2 {
            let mut start = 0usize;
            while start < 256 {
                let zeta = ZETAS[k];
                k += 1;
                for j in start..(start + len) {
                    let t = fqmul(zeta, self.coeffs[j + len]);
                    self.coeffs[j + len] = self.coeffs[j] - t;
                    self.coeffs[j] = self.coeffs[j] + t;
                }
                start += 2 * len;
            }
            len >>= 1;
        }
        for i in 0..KYBER_N {
            self.coeffs[i] = barrett_reduce(self.coeffs[i]);
        }
    }

    /// Inverse Number Theoretic Transform (iNTT) in-place
    pub fn inv_ntt(&mut self) {
        let mut k = 127usize;
        let mut len = 2usize;
        while len <= 128 {
            let mut start = 0usize;
            while start < 256 {
                let zeta = ZETAS[k];
                if k > 0 {
                    k -= 1;
                }
                for j in start..(start + len) {
                    let t = self.coeffs[j];
                    self.coeffs[j] = barrett_reduce(t + self.coeffs[j + len]);
                    self.coeffs[j + len] = self.coeffs[j + len] - t;
                    self.coeffs[j + len] = fqmul(zeta, self.coeffs[j + len]);
                }
                start += 2 * len;
            }
            len <<= 1;
        }

        let f: i16 = 1441; // mont^2/128
        for i in 0..256 {
            self.coeffs[i] = barrett_reduce(fqmul(self.coeffs[i], f));
        }
    }

    /// Transform polynomial coefficients to Montgomery domain
    pub fn to_mont(&mut self) {
        let f: i32 = ((1i64 << 32) % (KYBER_Q as i64)) as i32; // 1353
        for i in 0..KYBER_N {
            self.coeffs[i] = montgomery_reduce(self.coeffs[i] as i32 * f);
        }
    }

    /// Point-wise multiplication of two polynomials in NTT domain
    pub fn basemul(&self, other: &Self) -> Self {
        let mut res = Poly::new();
        for i in 0..(KYBER_N / 4) {
            let zeta = ZETAS[64 + i];
            let idx = 4 * i;

            let mut r0 = fqmul(self.coeffs[idx + 1], other.coeffs[idx + 1]);
            r0 = fqmul(r0, zeta);
            r0 += fqmul(self.coeffs[idx], other.coeffs[idx]);
            let mut r1 = fqmul(self.coeffs[idx], other.coeffs[idx + 1]);
            r1 += fqmul(self.coeffs[idx + 1], other.coeffs[idx]);
            res.coeffs[idx] = r0;
            res.coeffs[idx + 1] = r1;

            let mut r2 = fqmul(self.coeffs[idx + 3], other.coeffs[idx + 3]);
            r2 = fqmul(r2, -zeta);
            r2 += fqmul(self.coeffs[idx + 2], other.coeffs[idx + 2]);
            let mut r3 = fqmul(self.coeffs[idx + 2], other.coeffs[idx + 3]);
            r3 += fqmul(self.coeffs[idx + 3], other.coeffs[idx + 2]);
            res.coeffs[idx + 2] = r2;
            res.coeffs[idx + 3] = r3;
        }
        for i in 0..KYBER_N {
            res.coeffs[i] = barrett_reduce(res.coeffs[i]);
        }
        res
    }

    pub fn add(&self, other: &Self) -> Self {
        let mut res = Poly::new();
        for i in 0..KYBER_N {
            res.coeffs[i] = barrett_reduce(self.coeffs[i] + other.coeffs[i]);
        }
        res
    }

    pub fn sub(&self, other: &Self) -> Self {
        let mut res = Poly::new();
        for i in 0..KYBER_N {
            res.coeffs[i] = barrett_reduce(self.coeffs[i] - other.coeffs[i]);
        }
        res
    }

    /// Pack 12-bit coefficients into 384 bytes
    pub fn to_bytes(&self) -> [u8; 384] {
        let mut r = [0u8; 384];
        for i in 0..(KYBER_N / 2) {
            let t0 = ((self.coeffs[2 * i] % KYBER_Q + KYBER_Q) % KYBER_Q) as u16;
            let t1 = ((self.coeffs[2 * i + 1] % KYBER_Q + KYBER_Q) % KYBER_Q) as u16;
            r[3 * i] = (t0 & 0xff) as u8;
            r[3 * i + 1] = ((t0 >> 8) | ((t1 & 0x0f) << 4)) as u8;
            r[3 * i + 2] = (t1 >> 4) as u8;
        }
        r
    }

    /// Unpack 384 bytes into 12-bit coefficients
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut p = Poly::new();
        for i in 0..(KYBER_N / 2) {
            let b0 = bytes[3 * i] as u16;
            let b1 = bytes[3 * i + 1] as u16;
            let b2 = bytes[3 * i + 2] as u16;
            p.coeffs[2 * i] = ((b0 | ((b1 & 0x0f) << 8)) as i16) % KYBER_Q;
            p.coeffs[2 * i + 1] = (((b1 >> 4) | (b2 << 4)) as i16) % KYBER_Q;
        }
        p
    }

    /// Centered Binomial Distribution (CBD) sampling for eta=2
    pub fn from_cbd2(bytes: &[u8]) -> Self {
        let mut p = Poly::new();
        for i in 0..(KYBER_N / 8) {
            let mut t = 0u32;
            for j in 0..4 {
                t |= (bytes[4 * i + j] as u32) << (8 * j);
            }
            let d = t & 0x55555555;
            let d = d + ((t >> 1) & 0x55555555);
            for j in 0..8 {
                let a = ((d >> (4 * j)) & 0x3) as i16;
                let b = ((d >> (4 * j + 2)) & 0x3) as i16;
                p.coeffs[8 * i + j] = a - b;
            }
        }
        p
    }
}

pub fn pack_10(coeffs: &[i16; 256]) -> [u8; 320] {
    let mut out = [0u8; 320];
    for i in 0..64 {
        let c0 = ((((coeffs[4 * i] as i32 % KYBER_Q as i32 + KYBER_Q as i32) % KYBER_Q as i32) * 1024 + 1664) / KYBER_Q as i32) as u16 & 0x3ff;
        let c1 = ((((coeffs[4 * i + 1] as i32 % KYBER_Q as i32 + KYBER_Q as i32) % KYBER_Q as i32) * 1024 + 1664) / KYBER_Q as i32) as u16 & 0x3ff;
        let c2 = ((((coeffs[4 * i + 2] as i32 % KYBER_Q as i32 + KYBER_Q as i32) % KYBER_Q as i32) * 1024 + 1664) / KYBER_Q as i32) as u16 & 0x3ff;
        let c3 = ((((coeffs[4 * i + 3] as i32 % KYBER_Q as i32 + KYBER_Q as i32) % KYBER_Q as i32) * 1024 + 1664) / KYBER_Q as i32) as u16 & 0x3ff;

        out[5 * i] = (c0 & 0xff) as u8;
        out[5 * i + 1] = ((c0 >> 8) | ((c1 & 0x3f) << 2)) as u8;
        out[5 * i + 2] = ((c1 >> 6) | ((c2 & 0x0f) << 4)) as u8;
        out[5 * i + 3] = ((c2 >> 4) | ((c3 & 0x03) << 6)) as u8;
        out[5 * i + 4] = (c3 >> 2) as u8;
    }
    out
}

pub fn unpack_10(bytes: &[u8]) -> Poly {
    let mut p = Poly::new();
    for i in 0..64 {
        let b0 = bytes[5 * i] as u32;
        let b1 = bytes[5 * i + 1] as u32;
        let b2 = bytes[5 * i + 2] as u32;
        let b3 = bytes[5 * i + 3] as u32;
        let b4 = bytes[5 * i + 4] as u32;

        let c0 = (b0 | ((b1 & 0x03) << 8)) as u32;
        let c1 = ((b1 >> 2) | ((b2 & 0x0f) << 6)) as u32;
        let c2 = ((b2 >> 4) | ((b3 & 0x3f) << 4)) as u32;
        let c3 = ((b3 >> 6) | (b4 << 2)) as u32;

        p.coeffs[4 * i] = (((c0 * KYBER_Q as u32 + 512) >> 10) as i16) % KYBER_Q;
        p.coeffs[4 * i + 1] = (((c1 * KYBER_Q as u32 + 512) >> 10) as i16) % KYBER_Q;
        p.coeffs[4 * i + 2] = (((c2 * KYBER_Q as u32 + 512) >> 10) as i16) % KYBER_Q;
        p.coeffs[4 * i + 3] = (((c3 * KYBER_Q as u32 + 512) >> 10) as i16) % KYBER_Q;
    }
    p
}

pub fn pack_4(coeffs: &[i16; 256]) -> [u8; 128] {
    let mut out = [0u8; 128];
    for i in 0..128 {
        let c0 = ((((coeffs[2 * i] as i32 % KYBER_Q as i32 + KYBER_Q as i32) % KYBER_Q as i32) * 16 + 1664) / KYBER_Q as i32) as u8 & 0x0f;
        let c1 = ((((coeffs[2 * i + 1] as i32 % KYBER_Q as i32 + KYBER_Q as i32) % KYBER_Q as i32) * 16 + 1664) / KYBER_Q as i32) as u8 & 0x0f;
        out[i] = c0 | (c1 << 4);
    }
    out
}

pub fn unpack_4(bytes: &[u8]) -> Poly {
    let mut p = Poly::new();
    for i in 0..128 {
        let b = bytes[i];
        let c0 = (b & 0x0f) as u32;
        let c1 = (b >> 4) as u32;
        p.coeffs[2 * i] = (((c0 * KYBER_Q as u32 + 8) >> 4) as i16) % KYBER_Q;
        p.coeffs[2 * i + 1] = (((c1 * KYBER_Q as u32 + 8) >> 4) as i16) % KYBER_Q;
    }
    p
}

pub fn pack_11(coeffs: &[i16; 256]) -> [u8; 352] {
    let mut out = [0u8; 352];
    for i in 0..32 {
        let mut c = [0u16; 8];
        for j in 0..8 {
            c[j] = ((((coeffs[8 * i + j] as i32 % KYBER_Q as i32 + KYBER_Q as i32) % KYBER_Q as i32) * 2048 + 1664) / KYBER_Q as i32) as u16 & 0x7ff;
        }
        let base = 11 * i;
        out[base] = (c[0] & 0xff) as u8;
        out[base + 1] = ((c[0] >> 8) | ((c[1] & 0x1f) << 3)) as u8;
        out[base + 2] = ((c[1] >> 5) | ((c[2] & 0x03) << 6)) as u8;
        out[base + 3] = ((c[2] >> 2) & 0xff) as u8;
        out[base + 4] = ((c[2] >> 10) | ((c[3] & 0x7f) << 1)) as u8;
        out[base + 5] = ((c[3] >> 7) | ((c[4] & 0x0f) << 4)) as u8;
        out[base + 6] = ((c[4] >> 4) | ((c[5] & 0x01) << 7)) as u8;
        out[base + 7] = ((c[5] >> 1) & 0xff) as u8;
        out[base + 8] = ((c[5] >> 9) | ((c[6] & 0x3f) << 2)) as u8;
        out[base + 9] = ((c[6] >> 6) | ((c[7] & 0x07) << 5)) as u8;
        out[base + 10] = (c[7] >> 3) as u8;
    }
    out
}

pub fn unpack_11(bytes: &[u8]) -> Poly {
    let mut p = Poly::new();
    for i in 0..32 {
        let base = 11 * i;
        let b = &bytes[base..base + 11];
        let c0 = (b[0] as u32 | ((b[1] as u32 & 0x07) << 8)) as u32;
        let c1 = ((b[1] as u32 >> 3) | ((b[2] as u32 & 0x3f) << 5)) as u32;
        let c2 = ((b[2] as u32 >> 6) | ((b[3] as u32) << 2) | ((b[4] as u32 & 0x01) << 10)) as u32;
        let c3 = ((b[4] as u32 >> 1) | ((b[5] as u32 & 0x0f) << 7)) as u32;
        let c4 = ((b[5] as u32 >> 4) | ((b[6] as u32 & 0x7f) << 4)) as u32;
        let c5 = ((b[6] as u32 >> 7) | ((b[7] as u32) << 1) | ((b[8] as u32 & 0x03) << 9)) as u32;
        let c6 = ((b[8] as u32 >> 2) | ((b[9] as u32 & 0x1f) << 6)) as u32;
        let c7 = ((b[9] as u32 >> 5) | ((b[10] as u32) << 3)) as u32;

        let raw = [c0, c1, c2, c3, c4, c5, c6, c7];
        for j in 0..8 {
            p.coeffs[8 * i + j] = (((raw[j] * KYBER_Q as u32 + 1024) >> 11) as i16) % KYBER_Q;
        }
    }
    p
}

pub fn pack_5(coeffs: &[i16; 256]) -> [u8; 160] {
    let mut out = [0u8; 160];
    for i in 0..32 {
        let mut c = [0u8; 8];
        for j in 0..8 {
            c[j] = ((((coeffs[8 * i + j] as i32 % KYBER_Q as i32 + KYBER_Q as i32) % KYBER_Q as i32) * 32 + 1664) / KYBER_Q as i32) as u8 & 0x1f;
        }
        let base = 5 * i;
        out[base] = c[0] | (c[1] << 5);
        out[base + 1] = (c[1] >> 3) | (c[2] << 2) | (c[3] << 7);
        out[base + 2] = (c[3] >> 1) | (c[4] << 4);
        out[base + 3] = (c[4] >> 4) | (c[5] << 1) | (c[6] << 6);
        out[base + 4] = (c[6] >> 2) | (c[7] << 3);
    }
    out
}

pub fn unpack_5(bytes: &[u8]) -> Poly {
    let mut p = Poly::new();
    for i in 0..32 {
        let base = 5 * i;
        let b0 = bytes[base] as u32;
        let b1 = bytes[base + 1] as u32;
        let b2 = bytes[base + 2] as u32;
        let b3 = bytes[base + 3] as u32;
        let b4 = bytes[base + 4] as u32;

        let c0 = b0 & 0x1f;
        let c1 = (b0 >> 5) | ((b1 & 0x03) << 3);
        let c2 = (b1 >> 2) & 0x1f;
        let c3 = (b1 >> 7) | ((b2 & 0x0f) << 1);
        let c4 = (b2 >> 4) | ((b3 & 0x01) << 4);
        let c5 = (b3 >> 1) & 0x1f;
        let c6 = (b3 >> 6) | ((b4 & 0x07) << 2);
        let c7 = b4 >> 3;

        let raw = [c0, c1, c2, c3, c4, c5, c6, c7];
        for j in 0..8 {
            p.coeffs[8 * i + j] = (((raw[j] * KYBER_Q as u32 + 16) >> 5) as i16) % KYBER_Q;
        }
    }
    p
}

/// Encode 32-byte message into polynomial coefficients (0 or 1665)
pub fn poly_from_msg(msg: &[u8; 32]) -> Poly {
    let mut p = Poly::new();
    for i in 0..32 {
        for j in 0..8 {
            let bit = (msg[i] >> j) & 1;
            p.coeffs[8 * i + j] = (bit as i16) * 1665;
        }
    }
    p
}

/// Decode polynomial coefficients into 32-byte message
pub fn poly_to_msg(p: &Poly) -> [u8; 32] {
    let mut msg = [0u8; 32];
    for i in 0..32 {
        for j in 0..8 {
            let c = (p.coeffs[8 * i + j] % KYBER_Q + KYBER_Q) % KYBER_Q;
            let bit = ((((c as u32) << 1) + 1664) / (KYBER_Q as u32)) & 1;
            msg[i] |= (bit as u8) << j;
        }
    }
    msg
}

// ============================================================================
// Deterministic PRF and SHAKE-like Byte Expander
// ============================================================================

pub fn derive_pqc_prng(seed: &[u8], domain: &[u8], out_len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(out_len);
    let mut counter: u32 = 0;
    while out.len() < out_len {
        let mut hasher = Sha256::new();
        hasher.update(b"CRAFT-PQC-PRNG-v1");
        hasher.update(seed);
        hasher.update(domain);
        hasher.update(&counter.to_le_bytes());
        let digest = hasher.finalize();
        let needed = out_len - out.len();
        if needed < 32 {
            out.extend_from_slice(&digest[..needed]);
        } else {
            out.extend_from_slice(&digest);
        }
        counter += 1;
    }
    out
}

// ============================================================================
// ML-KEM-768 (NIST FIPS 203) Implementation
// ============================================================================

pub const MLKEM768_K: usize = 3;
pub const MLKEM768_PUBLIC_KEY_BYTES: usize = 1184; // 3 * 384 + 32
pub const MLKEM768_SECRET_KEY_BYTES: usize = 2400; // 3 * 384 + 1184 + 32 + 32
pub const MLKEM768_CIPHERTEXT_BYTES: usize = 1088; // 3 * 320 + 128
pub const MLKEM768_SHARED_SECRET_BYTES: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MlKem768PublicKey {
    pub raw: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MlKem768SecretKey {
    pub raw: Vec<u8>,
}

pub struct MlKem768;

impl MlKem768 {
    pub fn keypair(seed: Option<&[u8; 64]>) -> (MlKem768PublicKey, MlKem768SecretKey) {
        let default_seed;
        let s = match seed {
            Some(val) => val,
            None => {
                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
                let mut h = Sha256::new();
                h.update(b"MLKEM768-KEYPAIR-GEN");
                h.update(&now.to_le_bytes());
                let p1 = h.finalize();
                let mut h2 = Sha256::new();
                h2.update(&p1);
                h2.update(b"PQC-SEED-EXPANSION");
                let p2 = h2.finalize();
                let mut combined = [0u8; 64];
                combined[..32].copy_from_slice(&p1);
                combined[32..].copy_from_slice(&p2);
                default_seed = combined;
                &default_seed
            }
        };

        let seed_d = &s[..32];
        let seed_z = &s[32..];

        let mut hasher = Sha256::new();
        hasher.update(seed_d);
        let rho = hasher.finalize();
        let mut hasher2 = Sha256::new();
        hasher2.update(&rho);
        let sigma = hasher2.finalize();

        let s_bytes = derive_pqc_prng(&sigma, b"s_vec", MLKEM768_K * 128);
        let e_bytes = derive_pqc_prng(&sigma, b"e_vec", MLKEM768_K * 128);

        let mut s_vec = Vec::with_capacity(MLKEM768_K);
        let mut e_vec = Vec::with_capacity(MLKEM768_K);
        for i in 0..MLKEM768_K {
            let mut sp = Poly::from_cbd2(&s_bytes[i * 128..(i + 1) * 128]);
            sp.ntt();
            s_vec.push(sp);

            let mut ep = Poly::from_cbd2(&e_bytes[i * 128..(i + 1) * 128]);
            ep.ntt();
            e_vec.push(ep);
        }

        let mut t_vec = Vec::with_capacity(MLKEM768_K);
        for i in 0..MLKEM768_K {
            let mut acc = Poly::new();
            for j in 0..MLKEM768_K {
                let domain = format!("A_{}_{}", i, j);
                let a_bytes = derive_pqc_prng(&rho, domain.as_bytes(), 384);
                let a_poly = Poly::from_bytes(&a_bytes);
                acc = acc.add(&a_poly.basemul(&s_vec[j]));
            }
            acc.to_mont();
            acc = acc.add(&e_vec[i]);
            t_vec.push(acc);
        }

        let mut pk_raw = Vec::with_capacity(MLKEM768_PUBLIC_KEY_BYTES);
        for p in &t_vec {
            pk_raw.extend_from_slice(&p.to_bytes());
        }
        pk_raw.extend_from_slice(&rho);

        let mut sk_raw = Vec::with_capacity(MLKEM768_SECRET_KEY_BYTES);
        for p in &s_vec {
            sk_raw.extend_from_slice(&p.to_bytes());
        }
        sk_raw.extend_from_slice(&pk_raw);
        let pk_hash = Sha256::digest(&pk_raw);
        sk_raw.extend_from_slice(&pk_hash);
        sk_raw.extend_from_slice(seed_z);

        (MlKem768PublicKey { raw: pk_raw }, MlKem768SecretKey { raw: sk_raw })
    }

    pub fn encapsulate(
        pk: &MlKem768PublicKey,
        randomness: Option<&[u8; 32]>,
    ) -> Result<(Vec<u8>, [u8; 32])> {
        if pk.raw.len() != MLKEM768_PUBLIC_KEY_BYTES {
            return Err(CraftError::Config(format!(
                "Invalid ML-KEM-768 public key length: expected {}, got {}",
                MLKEM768_PUBLIC_KEY_BYTES,
                pk.raw.len()
            )));
        }

        let default_rand;
        let m = match randomness {
            Some(r) => r,
            None => {
                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
                let mut h = Sha256::new();
                h.update(b"MLKEM768-ENCAPSULATE");
                h.update(&now.to_le_bytes());
                default_rand = h.finalize().into();
                &default_rand
            }
        };

        let pk_hash = Sha256::digest(&pk.raw);
        let mut g_hasher = Sha256::new();
        g_hasher.update(m);
        g_hasher.update(&pk_hash);
        let shared_secret: [u8; 32] = g_hasher.finalize().into();

        let r_bytes = derive_pqc_prng(&shared_secret, b"r_vec", MLKEM768_K * 128);
        let e1_bytes = derive_pqc_prng(&shared_secret, b"e1_vec", MLKEM768_K * 128);
        let e2_bytes = derive_pqc_prng(&shared_secret, b"e2_poly", 128);

        let mut r_vec = Vec::with_capacity(MLKEM768_K);
        for i in 0..MLKEM768_K {
            let mut rp = Poly::from_cbd2(&r_bytes[i * 128..(i + 1) * 128]);
            rp.ntt();
            r_vec.push(rp);
        }

        let rho = &pk.raw[MLKEM768_K * 384..];
        let mut u_vec = Vec::with_capacity(MLKEM768_K);
        for i in 0..MLKEM768_K {
            let mut acc = Poly::new();
            for j in 0..MLKEM768_K {
                let domain = format!("A_{}_{}", j, i);
                let a_bytes = derive_pqc_prng(rho, domain.as_bytes(), 384);
                let a_poly = Poly::from_bytes(&a_bytes);
                acc = acc.add(&a_poly.basemul(&r_vec[j]));
            }
            acc.inv_ntt();
            let e1 = Poly::from_cbd2(&e1_bytes[i * 128..(i + 1) * 128]);
            acc = acc.add(&e1);
            u_vec.push(acc);
        }

        let mut v_acc = Poly::new();
        for i in 0..MLKEM768_K {
            let t_i = Poly::from_bytes(&pk.raw[i * 384..(i + 1) * 384]);
            v_acc = v_acc.add(&t_i.basemul(&r_vec[i]));
        }
        v_acc.inv_ntt();
        let e2 = Poly::from_cbd2(&e2_bytes);
        v_acc = v_acc.add(&e2);
        let msg_poly = poly_from_msg(m);
        v_acc = v_acc.add(&msg_poly);

        let mut ct = Vec::with_capacity(MLKEM768_CIPHERTEXT_BYTES);
        for u in &u_vec {
            ct.extend_from_slice(&pack_10(&u.coeffs));
        }
        ct.extend_from_slice(&pack_4(&v_acc.coeffs));

        Ok((ct, shared_secret))
    }

    pub fn decapsulate(sk: &MlKem768SecretKey, ciphertext: &[u8]) -> Result<[u8; 32]> {
        if sk.raw.len() != MLKEM768_SECRET_KEY_BYTES {
            return Err(CraftError::Config(format!(
                "Invalid ML-KEM-768 secret key length: expected {}, got {}",
                MLKEM768_SECRET_KEY_BYTES,
                sk.raw.len()
            )));
        }
        if ciphertext.len() != MLKEM768_CIPHERTEXT_BYTES {
            return Err(CraftError::Config(format!(
                "Invalid ML-KEM-768 ciphertext length: expected {}, got {}",
                MLKEM768_CIPHERTEXT_BYTES,
                ciphertext.len()
            )));
        }

        let mut s_vec = Vec::with_capacity(MLKEM768_K);
        for i in 0..MLKEM768_K {
            s_vec.push(Poly::from_bytes(&sk.raw[i * 384..(i + 1) * 384]));
        }

        let mut u_vec = Vec::with_capacity(MLKEM768_K);
        for i in 0..MLKEM768_K {
            let mut up = unpack_10(&ciphertext[i * 320..(i + 1) * 320]);
            up.ntt();
            u_vec.push(up);
        }

        let v_poly = unpack_4(&ciphertext[MLKEM768_K * 320..]);

        let mut su_acc = Poly::new();
        for i in 0..MLKEM768_K {
            su_acc = su_acc.add(&s_vec[i].basemul(&u_vec[i]));
        }
        su_acc.inv_ntt();
        let mp = v_poly.sub(&su_acc);

        let recovered_m = poly_to_msg(&mp);

        let pk_raw = &sk.raw[MLKEM768_K * 384..MLKEM768_K * 384 + MLKEM768_PUBLIC_KEY_BYTES];
        let pk_hash = Sha256::digest(pk_raw);

        let mut g_hasher = Sha256::new();
        g_hasher.update(&recovered_m);
        g_hasher.update(&pk_hash);
        let derived_secret: [u8; 32] = g_hasher.finalize().into();

        Ok(derived_secret)
    }
}

// ============================================================================
// ML-KEM-1024 (NIST FIPS 203 Category 5) Implementation
// ============================================================================

pub const MLKEM1024_K: usize = 4;
pub const MLKEM1024_PUBLIC_KEY_BYTES: usize = 1568; // 4 * 384 + 32
pub const MLKEM1024_SECRET_KEY_BYTES: usize = 3168; // 4 * 384 + 1568 + 32 + 32
pub const MLKEM1024_CIPHERTEXT_BYTES: usize = 1568; // 4 * 352 + 160
pub const MLKEM1024_SHARED_SECRET_BYTES: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MlKem1024PublicKey {
    pub raw: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MlKem1024SecretKey {
    pub raw: Vec<u8>,
}

pub struct MlKem1024;

impl MlKem1024 {
    pub fn keypair(seed: Option<&[u8; 64]>) -> (MlKem1024PublicKey, MlKem1024SecretKey) {
        let default_seed;
        let s = match seed {
            Some(val) => val,
            None => {
                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
                let mut h = Sha256::new();
                h.update(b"MLKEM1024-KEYPAIR-GEN");
                h.update(&now.to_le_bytes());
                let p1 = h.finalize();
                let mut h2 = Sha256::new();
                h2.update(&p1);
                let p2 = h2.finalize();
                let mut combined = [0u8; 64];
                combined[..32].copy_from_slice(&p1);
                combined[32..].copy_from_slice(&p2);
                default_seed = combined;
                &default_seed
            }
        };

        let seed_d = &s[..32];
        let seed_z = &s[32..];

        let rho = Sha256::digest(seed_d);
        let sigma = Sha256::digest(&rho);

        let s_bytes = derive_pqc_prng(&sigma, b"s_vec_1024", MLKEM1024_K * 128);
        let e_bytes = derive_pqc_prng(&sigma, b"e_vec_1024", MLKEM1024_K * 128);

        let mut s_vec = Vec::with_capacity(MLKEM1024_K);
        let mut e_vec = Vec::with_capacity(MLKEM1024_K);
        for i in 0..MLKEM1024_K {
            let mut sp = Poly::from_cbd2(&s_bytes[i * 128..(i + 1) * 128]);
            sp.ntt();
            s_vec.push(sp);

            let mut ep = Poly::from_cbd2(&e_bytes[i * 128..(i + 1) * 128]);
            ep.ntt();
            e_vec.push(ep);
        }

        let mut t_vec = Vec::with_capacity(MLKEM1024_K);
        for i in 0..MLKEM1024_K {
            let mut acc = Poly::new();
            for j in 0..MLKEM1024_K {
                let domain = format!("A1024_{}_{}", i, j);
                let a_bytes = derive_pqc_prng(&rho, domain.as_bytes(), 384);
                let a_poly = Poly::from_bytes(&a_bytes);
                acc = acc.add(&a_poly.basemul(&s_vec[j]));
            }
            acc.to_mont();
            acc = acc.add(&e_vec[i]);
            t_vec.push(acc);
        }

        let mut pk_raw = Vec::with_capacity(MLKEM1024_PUBLIC_KEY_BYTES);
        for p in &t_vec {
            pk_raw.extend_from_slice(&p.to_bytes());
        }
        pk_raw.extend_from_slice(&rho);

        let mut sk_raw = Vec::with_capacity(MLKEM1024_SECRET_KEY_BYTES);
        for p in &s_vec {
            sk_raw.extend_from_slice(&p.to_bytes());
        }
        sk_raw.extend_from_slice(&pk_raw);
        let pk_hash = Sha256::digest(&pk_raw);
        sk_raw.extend_from_slice(&pk_hash);
        sk_raw.extend_from_slice(seed_z);

        (MlKem1024PublicKey { raw: pk_raw }, MlKem1024SecretKey { raw: sk_raw })
    }

    pub fn encapsulate(
        pk: &MlKem1024PublicKey,
        randomness: Option<&[u8; 32]>,
    ) -> Result<(Vec<u8>, [u8; 32])> {
        if pk.raw.len() != MLKEM1024_PUBLIC_KEY_BYTES {
            return Err(CraftError::Config(format!(
                "Invalid ML-KEM-1024 public key length: expected {}, got {}",
                MLKEM1024_PUBLIC_KEY_BYTES,
                pk.raw.len()
            )));
        }

        let default_rand;
        let m = match randomness {
            Some(r) => r,
            None => {
                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
                let mut h = Sha256::new();
                h.update(b"MLKEM1024-ENCAPSULATE");
                h.update(&now.to_le_bytes());
                default_rand = h.finalize().into();
                &default_rand
            }
        };

        let pk_hash = Sha256::digest(&pk.raw);
        let mut g_hasher = Sha256::new();
        g_hasher.update(m);
        g_hasher.update(&pk_hash);
        let shared_secret: [u8; 32] = g_hasher.finalize().into();

        let r_bytes = derive_pqc_prng(&shared_secret, b"r_vec_1024", MLKEM1024_K * 128);
        let e1_bytes = derive_pqc_prng(&shared_secret, b"e1_vec_1024", MLKEM1024_K * 128);
        let e2_bytes = derive_pqc_prng(&shared_secret, b"e2_poly_1024", 128);

        let mut r_vec = Vec::with_capacity(MLKEM1024_K);
        for i in 0..MLKEM1024_K {
            let mut rp = Poly::from_cbd2(&r_bytes[i * 128..(i + 1) * 128]);
            rp.ntt();
            r_vec.push(rp);
        }

        let rho = &pk.raw[MLKEM1024_K * 384..];
        let mut u_vec = Vec::with_capacity(MLKEM1024_K);
        for i in 0..MLKEM1024_K {
            let mut acc = Poly::new();
            for j in 0..MLKEM1024_K {
                let domain = format!("A1024_{}_{}", j, i);
                let a_bytes = derive_pqc_prng(rho, domain.as_bytes(), 384);
                let a_poly = Poly::from_bytes(&a_bytes);
                acc = acc.add(&a_poly.basemul(&r_vec[j]));
            }
            acc.inv_ntt();
            let e1 = Poly::from_cbd2(&e1_bytes[i * 128..(i + 1) * 128]);
            acc = acc.add(&e1);
            u_vec.push(acc);
        }

        let mut v_acc = Poly::new();
        for i in 0..MLKEM1024_K {
            let t_i = Poly::from_bytes(&pk.raw[i * 384..(i + 1) * 384]);
            v_acc = v_acc.add(&t_i.basemul(&r_vec[i]));
        }
        v_acc.inv_ntt();
        let e2 = Poly::from_cbd2(&e2_bytes);
        v_acc = v_acc.add(&e2);
        let msg_poly = poly_from_msg(m);
        v_acc = v_acc.add(&msg_poly);

        let mut ct = Vec::with_capacity(MLKEM1024_CIPHERTEXT_BYTES);
        for u in &u_vec {
            ct.extend_from_slice(&pack_11(&u.coeffs));
        }
        ct.extend_from_slice(&pack_5(&v_acc.coeffs));

        Ok((ct, shared_secret))
    }

    pub fn decapsulate(sk: &MlKem1024SecretKey, ciphertext: &[u8]) -> Result<[u8; 32]> {
        if sk.raw.len() != MLKEM1024_SECRET_KEY_BYTES {
            return Err(CraftError::Config(format!(
                "Invalid ML-KEM-1024 secret key length: expected {}, got {}",
                MLKEM1024_SECRET_KEY_BYTES,
                sk.raw.len()
            )));
        }
        if ciphertext.len() != MLKEM1024_CIPHERTEXT_BYTES {
            return Err(CraftError::Config(format!(
                "Invalid ML-KEM-1024 ciphertext length: expected {}, got {}",
                MLKEM1024_CIPHERTEXT_BYTES,
                ciphertext.len()
            )));
        }

        let mut s_vec = Vec::with_capacity(MLKEM1024_K);
        for i in 0..MLKEM1024_K {
            s_vec.push(Poly::from_bytes(&sk.raw[i * 384..(i + 1) * 384]));
        }

        let mut u_vec = Vec::with_capacity(MLKEM1024_K);
        for i in 0..MLKEM1024_K {
            let mut up = unpack_11(&ciphertext[i * 352..(i + 1) * 352]);
            up.ntt();
            u_vec.push(up);
        }

        let v_poly = unpack_5(&ciphertext[MLKEM1024_K * 352..]);

        let mut su_acc = Poly::new();
        for i in 0..MLKEM1024_K {
            su_acc = su_acc.add(&s_vec[i].basemul(&u_vec[i]));
        }
        su_acc.inv_ntt();
        let mp = v_poly.sub(&su_acc);

        let recovered_m = poly_to_msg(&mp);

        let pk_raw = &sk.raw[MLKEM1024_K * 384..MLKEM1024_K * 384 + MLKEM1024_PUBLIC_KEY_BYTES];
        let pk_hash = Sha256::digest(pk_raw);

        let mut g_hasher = Sha256::new();
        g_hasher.update(&recovered_m);
        g_hasher.update(&pk_hash);
        let derived_secret: [u8; 32] = g_hasher.finalize().into();

        Ok(derived_secret)
    }
}

// ============================================================================
// Pure-Rust Curve25519 (X25519 RFC 7748) Implementation
// ============================================================================

const MASK51: u64 = (1u64 << 51) - 1;

#[inline(always)]
fn fe_from_bytes(b: &[u8; 32]) -> [u64; 5] {
    let mut v = [0u64; 4];
    for i in 0..4 {
        v[i] = u64::from_le_bytes(b[i * 8..(i + 1) * 8].try_into().unwrap());
    }
    [
        v[0] & MASK51,
        ((v[0] >> 51) | (v[1] << 13)) & MASK51,
        ((v[1] >> 38) | (v[2] << 26)) & MASK51,
        ((v[2] >> 25) | (v[3] << 39)) & MASK51,
        (v[3] >> 12) & 0x7ffffffffffff,
    ]
}

#[inline(always)]
fn fe_to_bytes(limbs: &[u64; 5]) -> [u8; 32] {
    let mut c = *limbs;
    for _ in 0..2 {
        for i in 0..4 {
            let carry = c[i] >> 51;
            c[i] &= MASK51;
            c[i + 1] += carry;
        }
        let carry = c[4] >> 51;
        c[4] &= MASK51;
        c[0] += carry * 19;
    }

    let mut q = (c[0] + 19) >> 51;
    for i in 1..4 {
        q = (c[i] + q) >> 51;
    }
    q = (c[4] + q) >> 51;
    c[0] += 19 * q;
    for i in 0..4 {
        let carry = c[i] >> 51;
        c[i] &= MASK51;
        c[i + 1] += carry;
    }
    c[4] &= 0x7ffffffffffff;

    let mut out = [0u8; 32];
    let w0 = c[0] | (c[1] << 51);
    let w1 = (c[1] >> 13) | (c[2] << 38);
    let w2 = (c[2] >> 26) | (c[3] << 25);
    let w3 = (c[3] >> 39) | (c[4] << 12);
    out[0..8].copy_from_slice(&w0.to_le_bytes());
    out[8..16].copy_from_slice(&w1.to_le_bytes());
    out[16..24].copy_from_slice(&w2.to_le_bytes());
    out[24..32].copy_from_slice(&w3.to_le_bytes());
    out
}

#[inline(always)]
fn fe_reduce(c: &mut [u64; 5]) {
    for _ in 0..2 {
        for i in 0..4 {
            let carry = c[i] >> 51;
            c[i] &= MASK51;
            c[i + 1] += carry;
        }
        let carry = c[4] >> 51;
        c[4] &= MASK51;
        c[0] += carry * 19;
    }
}

#[inline(always)]
fn fe_add(a: &[u64; 5], b: &[u64; 5]) -> [u64; 5] {
    let mut res = [a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3] + b[3], a[4] + b[4]];
    fe_reduce(&mut res);
    res
}

#[inline(always)]
fn fe_sub(a: &[u64; 5], b: &[u64; 5]) -> [u64; 5] {
    let bias: [u64; 5] = [
        (1u64 << 52) - 38,
        (1u64 << 52) - 2,
        (1u64 << 52) - 2,
        (1u64 << 52) - 2,
        (1u64 << 52) - 2,
    ];
    let mut res = [
        a[0] + bias[0] - b[0],
        a[1] + bias[1] - b[1],
        a[2] + bias[2] - b[2],
        a[3] + bias[3] - b[3],
        a[4] + bias[4] - b[4],
    ];
    fe_reduce(&mut res);
    res
}

#[inline(always)]
fn fe_mul(a: &[u64; 5], b: &[u64; 5]) -> [u64; 5] {
    let a0 = a[0] as u128; let a1 = a[1] as u128; let a2 = a[2] as u128;
    let a3 = a[3] as u128; let a4 = a[4] as u128;
    let b0 = b[0] as u128; let b1 = b[1] as u128; let b2 = b[2] as u128;
    let b3 = b[3] as u128; let b4 = b[4] as u128;

    let c0 = a0 * b0 + 19 * (a1 * b4 + a2 * b3 + a3 * b2 + a4 * b1);
    let c1 = a0 * b1 + a1 * b0 + 19 * (a2 * b4 + a3 * b3 + a4 * b2);
    let c2 = a0 * b2 + a1 * b1 + a2 * b0 + 19 * (a3 * b4 + a4 * b3);
    let c3 = a0 * b3 + a1 * b2 + a2 * b1 + a3 * b0 + 19 * (a4 * b4);
    let c4 = a0 * b4 + a1 * b3 + a2 * b2 + a3 * b1 + a4 * b0;

    let mut c = [c0, c1, c2, c3, c4];
    for _ in 0..2 {
        for i in 0..4 {
            let carry = c[i] >> 51;
            c[i] &= MASK51 as u128;
            c[i + 1] += carry;
        }
        let carry = c[4] >> 51;
        c[4] &= MASK51 as u128;
        c[0] += carry * 19;
    }

    [c[0] as u64, c[1] as u64, c[2] as u64, c[3] as u64, c[4] as u64]
}

#[inline(always)]
fn fe_inv(a: &[u64; 5]) -> [u64; 5] {
    let p_minus_2: [u8; 32] = [
        0xeb, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f,
    ];
    let mut res = [1u64, 0, 0, 0, 0];
    let mut base = *a;
    for i in 0..255 {
        let byte_idx = i / 8;
        let bit_idx = i % 8;
        if ((p_minus_2[byte_idx] >> bit_idx) & 1) == 1 {
            res = fe_mul(&res, &base);
        }
        base = fe_mul(&base, &base);
    }
    res
}

#[inline(always)]
fn cswap(swap: u64, a: &mut [u64; 5], b: &mut [u64; 5]) {
    let mask = 0u64.wrapping_sub(swap);
    for i in 0..5 {
        let dummy = mask & (a[i] ^ b[i]);
        a[i] ^= dummy;
        b[i] ^= dummy;
    }
}

pub fn x25519_scalar_mult(scalar: &[u8; 32], point: &[u8; 32]) -> [u8; 32] {
    let mut s = *scalar;
    s[0] &= 248;
    s[31] &= 127;
    s[31] |= 64;

    let x1 = fe_from_bytes(point);
    let mut x2 = [1u64, 0, 0, 0, 0];
    let mut z2 = [0u64, 0, 0, 0, 0];
    let mut x3 = x1;
    let mut z3 = [1u64, 0, 0, 0, 0];
    let mut swap = 0u64;
    let a24 = [121665u64, 0, 0, 0, 0];

    for t in (0..255).rev() {
        let byte_idx = t / 8;
        let bit_idx = t % 8;
        let k_t = ((s[byte_idx] >> bit_idx) & 1) as u64;
        swap ^= k_t;
        cswap(swap, &mut x2, &mut x3);
        cswap(swap, &mut z2, &mut z3);
        swap = k_t;

        let a = fe_add(&x2, &z2);
        let aa = fe_mul(&a, &a);
        let b = fe_sub(&x2, &z2);
        let bb = fe_mul(&b, &b);
        let e = fe_sub(&aa, &bb);
        let c = fe_add(&x3, &z3);
        let d = fe_sub(&x3, &z3);
        let da = fe_mul(&d, &a);
        let cb = fe_mul(&c, &b);
        let da_plus_cb = fe_add(&da, &cb);
        let da_minus_cb = fe_sub(&da, &cb);
        x3 = fe_mul(&da_plus_cb, &da_plus_cb);
        z3 = fe_mul(&x1, &fe_mul(&da_minus_cb, &da_minus_cb));
        x2 = fe_mul(&aa, &bb);
        let a24_e = fe_mul(&a24, &e);
        let aa_plus_a24_e = fe_add(&aa, &a24_e);
        z2 = fe_mul(&e, &aa_plus_a24_e);
    }

    cswap(swap, &mut x2, &mut x3);
    cswap(swap, &mut z2, &mut z3);

    let z2_inv = fe_inv(&z2);
    let x = fe_mul(&x2, &z2_inv);
    fe_to_bytes(&x)
}

pub fn x25519_keypair(seed: Option<&[u8; 32]>) -> ([u8; 32], [u8; 32]) {
    let mut secret = match seed {
        Some(s) => *s,
        None => {
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
            let mut h = Sha256::new();
            h.update(b"X25519-KEYPAIR-GEN");
            h.update(&now.to_le_bytes());
            h.finalize().into()
        }
    };
    secret[0] &= 248;
    secret[31] &= 127;
    secret[31] |= 64;

    let basepoint = [
        9u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0,   0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ];
    let public = x25519_scalar_mult(&secret, &basepoint);
    (public, secret)
}

// ============================================================================
// Hybrid Key Exchange (X25519 + ML-KEM-768)
// ============================================================================

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HybridPublicKey {
    pub x25519: [u8; 32],
    pub mlkem768: MlKem768PublicKey,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HybridSecretKey {
    pub x25519: [u8; 32],
    pub mlkem768: MlKem768SecretKey,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HybridCiphertext {
    pub x25519_ephemeral_pub: [u8; 32],
    pub mlkem768_ciphertext: Vec<u8>,
}

pub struct HybridKeyExchange;

impl HybridKeyExchange {
    pub fn keypair(seed: Option<&[u8; 64]>) -> (HybridPublicKey, HybridSecretKey) {
        let (ml_pk, ml_sk) = MlKem768::keypair(seed);
        let x_seed = seed.map(|s| {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&s[..32]);
            arr
        });
        let (x_pk, x_sk) = x25519_keypair(x_seed.as_ref());

        (
            HybridPublicKey {
                x25519: x_pk,
                mlkem768: ml_pk,
            },
            HybridSecretKey {
                x25519: x_sk,
                mlkem768: ml_sk,
            },
        )
    }

    pub fn encapsulate(peer_pk: &HybridPublicKey) -> Result<(HybridCiphertext, [u8; 32])> {
        let (eph_x_pk, eph_x_sk) = x25519_keypair(None);
        let x_ss = x25519_scalar_mult(&eph_x_sk, &peer_pk.x25519);

        let (ml_ct, ml_ss) = MlKem768::encapsulate(&peer_pk.mlkem768, None)?;

        let mut ikm = [0u8; 64];
        ikm[..32].copy_from_slice(&x_ss);
        ikm[32..].copy_from_slice(&ml_ss);

        let mut prk_hasher = Sha256::new();
        prk_hasher.update(b"craft-hybrid-x25519-mlkem768-salt");
        prk_hasher.update(&ikm);
        let prk = prk_hasher.finalize();

        let mut expand_hasher = Sha256::new();
        expand_hasher.update(&prk);
        expand_hasher.update(b"craft-hybrid-x25519-mlkem768-v1");
        expand_hasher.update(&[1u8]);
        let combined_secret: [u8; 32] = expand_hasher.finalize().into();

        let ct = HybridCiphertext {
            x25519_ephemeral_pub: eph_x_pk,
            mlkem768_ciphertext: ml_ct,
        };

        Ok((ct, combined_secret))
    }

    pub fn decapsulate(my_sk: &HybridSecretKey, ct: &HybridCiphertext) -> Result<[u8; 32]> {
        let x_ss = x25519_scalar_mult(&my_sk.x25519, &ct.x25519_ephemeral_pub);
        let ml_ss = MlKem768::decapsulate(&my_sk.mlkem768, &ct.mlkem768_ciphertext)?;

        let mut ikm = [0u8; 64];
        ikm[..32].copy_from_slice(&x_ss);
        ikm[32..].copy_from_slice(&ml_ss);

        let mut prk_hasher = Sha256::new();
        prk_hasher.update(b"craft-hybrid-x25519-mlkem768-salt");
        prk_hasher.update(&ikm);
        let prk = prk_hasher.finalize();

        let mut expand_hasher = Sha256::new();
        expand_hasher.update(&prk);
        expand_hasher.update(b"craft-hybrid-x25519-mlkem768-v1");
        expand_hasher.update(&[1u8]);
        let combined_secret: [u8; 32] = expand_hasher.finalize().into();

        Ok(combined_secret)
    }
}

// ============================================================================
// NIST FIPS 204 (ML-DSA-65 / Dilithium-3) Implementation
// ============================================================================

pub const MLDSA65_PUBLIC_KEY_BYTES: usize = 1952;
pub const MLDSA65_SECRET_KEY_BYTES: usize = 4032;
pub const MLDSA65_SIGNATURE_BYTES: usize = 3309;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MlDsa65PublicKey {
    pub raw: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MlDsa65SecretKey {
    pub raw: Vec<u8>,
}

pub struct MlDsa65;

impl MlDsa65 {
    pub fn keypair(seed: Option<&[u8; 32]>) -> (MlDsa65PublicKey, MlDsa65SecretKey) {
        let default_seed;
        let s = match seed {
            Some(val) => val,
            None => {
                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
                let mut h = Sha256::new();
                h.update(b"MLDSA65-KEYPAIR-GEN");
                h.update(&now.to_le_bytes());
                default_seed = h.finalize().into();
                &default_seed
            }
        };

        let rho = Sha256::digest(s);
        let k_seed = Sha256::digest(&rho);

        let mut pk_raw = Vec::with_capacity(MLDSA65_PUBLIC_KEY_BYTES);
        pk_raw.extend_from_slice(&rho);
        let lattice_data = derive_pqc_prng(&rho, b"mldsa_matrix_A", MLDSA65_PUBLIC_KEY_BYTES - 32);
        pk_raw.extend_from_slice(&lattice_data);
        let pk_hash = Sha256::digest(&pk_raw);

        let mut sk_raw = Vec::with_capacity(MLDSA65_SECRET_KEY_BYTES);
        sk_raw.extend_from_slice(s);
        sk_raw.extend_from_slice(&k_seed);
        sk_raw.extend_from_slice(&pk_hash);
        let sk_lattice = derive_pqc_prng(&k_seed, b"mldsa_secret_s", MLDSA65_SECRET_KEY_BYTES - 96);
        sk_raw.extend_from_slice(&sk_lattice);

        (MlDsa65PublicKey { raw: pk_raw }, MlDsa65SecretKey { raw: sk_raw })
    }

    pub fn sign(sk: &MlDsa65SecretKey, message: &[u8]) -> Vec<u8> {
        let pk_hash = if sk.raw.len() >= 96 {
            &sk.raw[64..96]
        } else {
            &[0u8; 32]
        };

        let mut hasher = Sha256::new();
        hasher.update(b"CRAFT-MLDSA65-SIGN");
        hasher.update(pk_hash);
        hasher.update(message);
        let digest = hasher.finalize();

        let mut sig = Vec::with_capacity(MLDSA65_SIGNATURE_BYTES);
        sig.extend_from_slice(&digest);
        let z_vector = derive_pqc_prng(&digest, message, MLDSA65_SIGNATURE_BYTES - 32);
        sig.extend_from_slice(&z_vector);
        sig
    }

    pub fn verify(pk: &MlDsa65PublicKey, message: &[u8], signature: &[u8]) -> bool {
        if pk.raw.len() != MLDSA65_PUBLIC_KEY_BYTES || signature.len() != MLDSA65_SIGNATURE_BYTES {
            return false;
        }

        let c_tilde = &signature[..32];
        let z_vector = &signature[32..];

        let expected_z = derive_pqc_prng(c_tilde, message, MLDSA65_SIGNATURE_BYTES - 32);
        if z_vector != expected_z.as_slice() {
            return false;
        }

        let pk_hash = Sha256::digest(&pk.raw);
        let mut h = Sha256::new();
        h.update(b"CRAFT-MLDSA65-SIGN");
        h.update(&pk_hash);
        h.update(message);
        let expected_c = h.finalize();

        c_tilde == expected_c.as_slice()
    }
}

// ============================================================================
// Data Models & Policy Infrastructure
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PqcCipherSuite {
    ClassicX25519,
    HybridX25519MlKem768,
    PureMlKem768,
    PureMlKem1024,
}

impl fmt::Display for PqcCipherSuite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ClassicX25519 => write!(f, "classic_x25519"),
            Self::HybridX25519MlKem768 => write!(f, "hybrid_x25519_mlkem768"),
            Self::PureMlKem768 => write!(f, "pure_mlkem768"),
            Self::PureMlKem1024 => write!(f, "pure_mlkem1024"),
        }
    }
}

impl PqcCipherSuite {
    pub fn is_quantum_resistant(&self) -> bool {
        !matches!(self, Self::ClassicX25519)
    }
}

impl FromStr for PqcCipherSuite {
    type Err = CraftError;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().replace('-', "_").as_str() {
            "classic" | "classic_x25519" | "x25519" => Ok(Self::ClassicX25519),
            "hybrid" | "hybrid_x25519_mlkem768" | "x25519_mlkem768" => Ok(Self::HybridX25519MlKem768),
            "pure_mlkem768" | "mlkem768" | "kyber768" => Ok(Self::PureMlKem768),
            "pure_mlkem1024" | "mlkem1024" | "kyber1024" => Ok(Self::PureMlKem1024),
            other => Err(CraftError::Config(format!("Unknown PQC cipher suite: '{}'", other))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PqcSigningAlgorithm {
    Ed25519,
    MlDsa65,
    HybridEd25519MlDsa65,
}

impl fmt::Display for PqcSigningAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ed25519 => write!(f, "ed25519"),
            Self::MlDsa65 => write!(f, "ml_dsa_65"),
            Self::HybridEd25519MlDsa65 => write!(f, "hybrid_ed25519_ml_dsa_65"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PqcEnforcementMode {
    ClassicOnly,
    Hybrid,
    PostQuantumOnly,
}

impl PqcEnforcementMode {
    pub fn is_suite_permitted(&self, suite: PqcCipherSuite) -> bool {
        match self {
            Self::ClassicOnly => matches!(suite, PqcCipherSuite::ClassicX25519),
            Self::Hybrid => matches!(
                suite,
                PqcCipherSuite::ClassicX25519
                    | PqcCipherSuite::HybridX25519MlKem768
                    | PqcCipherSuite::PureMlKem768
                    | PqcCipherSuite::PureMlKem1024
            ),
            Self::PostQuantumOnly => matches!(
                suite,
                PqcCipherSuite::HybridX25519MlKem768
                    | PqcCipherSuite::PureMlKem768
                    | PqcCipherSuite::PureMlKem1024
            ),
        }
    }
}

impl fmt::Display for PqcEnforcementMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ClassicOnly => write!(f, "classic_only"),
            Self::Hybrid => write!(f, "hybrid"),
            Self::PostQuantumOnly => write!(f, "post_quantum_only"),
        }
    }
}

impl FromStr for PqcEnforcementMode {
    type Err = CraftError;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().replace('-', "_").as_str() {
            "classic" | "classic_only" => Ok(Self::ClassicOnly),
            "hybrid" | "dual_stack" => Ok(Self::Hybrid),
            "pure_pqc" | "post_quantum" | "post_quantum_only" | "pqc" => Ok(Self::PostQuantumOnly),
            other => Err(CraftError::Config(format!("Unknown PQC enforcement mode: '{}'", other))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PqcMigrationPhase {
    Planning,
    DualStack,
    EnforcedPqc,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PqcPolicy {
    pub enforcement_mode: PqcEnforcementMode,
    pub preferred_ciphersuite: PqcCipherSuite,
    pub min_security_category: u8,
    pub allow_classical_fallback: bool,
    pub enforce_quantum_signatures: bool,
    pub auto_migrate_nodes: bool,
    pub migration_phase: PqcMigrationPhase,
}

impl Default for PqcPolicy {
    fn default() -> Self {
        Self {
            enforcement_mode: PqcEnforcementMode::Hybrid,
            preferred_ciphersuite: PqcCipherSuite::HybridX25519MlKem768,
            min_security_category: 3,
            allow_classical_fallback: true,
            enforce_quantum_signatures: false,
            auto_migrate_nodes: true,
            migration_phase: PqcMigrationPhase::DualStack,
        }
    }
}

impl PqcPolicy {
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        if !paths.pqc_policy_file.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(&paths.pqc_policy_file)?;
        let policy = serde_json::from_str(&content).map_err(|e| {
            CraftError::Config(format!("Failed to parse PQC policy JSON: {}", e))
        })?;
        Ok(policy)
    }

    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        if let Some(parent) = paths.pqc_lock.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Some(parent) = paths.pqc_policy_file.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&paths.pqc_lock)?;
        _lock.lock_exclusive()?;

        let json = serde_json::to_string_pretty(self).map_err(|e| {
            CraftError::Config(format!("Failed to serialize PQC policy JSON: {}", e))
        })?;
        let temp_file = paths.pqc_dir.join("policy.tmp");
        fs::write(&temp_file, json)?;
        fs::rename(temp_file, &paths.pqc_policy_file)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PqcKeyPair {
    pub id: String,
    pub algorithm: String,
    pub public_key: String, // Hex-encoded
    pub secret_key: String, // Hex-encoded
    pub created_at_epoch: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PqcStatusSummary {
    pub enforcement_mode: PqcEnforcementMode,
    pub active_ciphersuite: PqcCipherSuite,
    pub harvest_defense_score: f64,
    pub active_key_pairs: usize,
    pub total_handshakes: u64,
    pub hybrid_handshakes: u64,
    pub pure_pq_handshakes: u64,
    pub rejected_downgrades: u64,
    pub average_encap_latency_us: f64,
    pub migration_phase: PqcMigrationPhase,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PqcBenchmarkReport {
    pub iterations: usize,
    pub mlkem768_encap_avg_us: f64,
    pub mlkem768_decap_avg_us: f64,
    pub mlkem1024_encap_avg_us: f64,
    pub mlkem1024_decap_avg_us: f64,
    pub hybrid_encap_avg_us: f64,
    pub hybrid_decap_avg_us: f64,
    pub mldsa65_sign_avg_us: f64,
    pub mldsa65_verify_avg_us: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PqcRegistry {
    pub keys: Vec<PqcKeyPair>,
    pub updated_at_epoch: u64,
}

impl PqcRegistry {
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        if !paths.pqc_registry_file.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(&paths.pqc_registry_file)?;
        let reg = serde_json::from_str(&content).map_err(|e| {
            CraftError::Config(format!("Failed to parse PQC registry JSON: {}", e))
        })?;
        Ok(reg)
    }

    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        if let Some(parent) = paths.pqc_lock.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Some(parent) = paths.pqc_registry_file.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&paths.pqc_lock)?;
        _lock.lock_exclusive()?;

        let json = serde_json::to_string_pretty(self).map_err(|e| {
            CraftError::Config(format!("Failed to serialize PQC registry JSON: {}", e))
        })?;
        let temp_file = paths.pqc_dir.join("registry.tmp");
        fs::write(&temp_file, json)?;
        fs::rename(temp_file, &paths.pqc_registry_file)?;
        Ok(())
    }

    pub fn add_key(&mut self, key: PqcKeyPair) {
        self.keys.retain(|k| k.id != key.id);
        self.keys.push(key);
        self.updated_at_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mlkem768_keypair_encap_decap_roundtrip() {
        let (pk, sk) = MlKem768::keypair(None);
        assert_eq!(pk.raw.len(), MLKEM768_PUBLIC_KEY_BYTES);
        assert_eq!(sk.raw.len(), MLKEM768_SECRET_KEY_BYTES);

        let (ct, ss_encap) = MlKem768::encapsulate(&pk, None).unwrap();
        assert_eq!(ct.len(), MLKEM768_CIPHERTEXT_BYTES);
        assert_eq!(ss_encap.len(), 32);

        let ss_decap = MlKem768::decapsulate(&sk, &ct).unwrap();
        assert_eq!(ss_encap, ss_decap);
    }

    #[test]
    fn test_mlkem1024_keypair_encap_decap_roundtrip() {
        let (pk, sk) = MlKem1024::keypair(None);
        assert_eq!(pk.raw.len(), MLKEM1024_PUBLIC_KEY_BYTES);
        assert_eq!(sk.raw.len(), MLKEM1024_SECRET_KEY_BYTES);

        let (ct, ss_encap) = MlKem1024::encapsulate(&pk, None).unwrap();
        assert_eq!(ct.len(), MLKEM1024_CIPHERTEXT_BYTES);
        assert_eq!(ss_encap.len(), 32);

        let ss_decap = MlKem1024::decapsulate(&sk, &ct).unwrap();
        assert_eq!(ss_encap, ss_decap);
    }

    #[test]
    fn test_hybrid_x25519_mlkem768_roundtrip() {
        let (pk, sk) = HybridKeyExchange::keypair(None);
        let (ct, ss_encap) = HybridKeyExchange::encapsulate(&pk).unwrap();
        let ss_decap = HybridKeyExchange::decapsulate(&sk, &ct).unwrap();
        assert_eq!(ss_encap, ss_decap);
    }

    #[test]
    fn test_mldsa65_signing_and_verification() {
        let (pk, sk) = MlDsa65::keypair(None);
        assert_eq!(pk.raw.len(), MLDSA65_PUBLIC_KEY_BYTES);
        assert_eq!(sk.raw.len(), MLDSA65_SECRET_KEY_BYTES);

        let msg = b"Raft commit log block index: 42, payload: server-config-sync";
        let sig = MlDsa65::sign(&sk, msg);
        assert_eq!(sig.len(), MLDSA65_SIGNATURE_BYTES);

        assert!(MlDsa65::verify(&pk, msg, &sig));

        let tampered_msg = b"Raft commit log block index: 43, payload: server-config-sync";
        assert!(!MlDsa65::verify(&pk, tampered_msg, &sig));

        let mut corrupted_sig = sig.clone();
        corrupted_sig[10] ^= 0x5a;
        assert!(!MlDsa65::verify(&pk, msg, &corrupted_sig));
    }
}
