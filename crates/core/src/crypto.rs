use crate::audit::compute_hmac_sha256;
use crate::error::{CraftError, Result};

pub struct ChaCha20 {
    state: [u32; 16],
}

impl ChaCha20 {
    pub fn new(key: &[u8; 32], nonce: &[u8; 12], counter: u32) -> Self {
        let mut state = [0u32; 16];
        state[0] = 0x61707865;
        state[1] = 0x3320646e;
        state[2] = 0x79622d32;
        state[3] = 0x6b206574;

        for i in 0..8 {
            let chunk: [u8; 4] = key[i * 4..(i + 1) * 4].try_into().unwrap();
            state[4 + i] = u32::from_le_bytes(chunk);
        }

        state[12] = counter;

        for i in 0..3 {
            let chunk: [u8; 4] = nonce[i * 4..(i + 1) * 4].try_into().unwrap();
            state[13 + i] = u32::from_le_bytes(chunk);
        }

        Self { state }
    }

    #[inline(always)]
    fn quarter_round(s: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
        s[a] = s[a].wrapping_add(s[b]);
        s[d] ^= s[a];
        s[d] = s[d].rotate_left(16);

        s[c] = s[c].wrapping_add(s[d]);
        s[b] ^= s[c];
        s[b] = s[b].rotate_left(12);

        s[a] = s[a].wrapping_add(s[b]);
        s[d] ^= s[a];
        s[d] = s[d].rotate_left(8);

        s[c] = s[c].wrapping_add(s[d]);
        s[b] ^= s[c];
        s[b] = s[b].rotate_left(7);
    }

    pub fn block(&self, counter: u32) -> [u8; 64] {
        let mut s = self.state;
        s[12] = counter;
        let initial = s;

        for _ in 0..10 {
            // Column round
            Self::quarter_round(&mut s, 0, 4, 8, 12);
            Self::quarter_round(&mut s, 1, 5, 9, 13);
            Self::quarter_round(&mut s, 2, 6, 10, 14);
            Self::quarter_round(&mut s, 3, 7, 11, 15);

            // Diagonal round
            Self::quarter_round(&mut s, 0, 5, 10, 15);
            Self::quarter_round(&mut s, 1, 6, 11, 12);
            Self::quarter_round(&mut s, 2, 7, 8, 13);
            Self::quarter_round(&mut s, 3, 4, 9, 14);
        }

        let mut out = [0u8; 64];
        for i in 0..16 {
            let res = s[i].wrapping_add(initial[i]);
            out[i * 4..(i + 1) * 4].copy_from_slice(&res.to_le_bytes());
        }
        out
    }

    pub fn process(&self, mut counter: u32, data: &[u8]) -> Vec<u8> {
        let mut result = Vec::with_capacity(data.len());
        let mut offset = 0;

        while offset < data.len() {
            let block = self.block(counter);
            counter = counter.wrapping_add(1);

            let len = (data.len() - offset).min(64);
            for i in 0..len {
                result.push(data[offset + i] ^ block[i]);
            }
            offset += len;
        }

        result
    }
}

pub struct Poly1305;

impl Poly1305 {
    pub fn mac(key: &[u8; 32], msg: &[u8]) -> [u8; 16] {
        let k0 = u32::from_le_bytes(key[0..4].try_into().unwrap());
        let k1 = u32::from_le_bytes(key[4..8].try_into().unwrap());
        let k2 = u32::from_le_bytes(key[8..12].try_into().unwrap());
        let k3 = u32::from_le_bytes(key[12..16].try_into().unwrap());

        // Clamping r
        let r0 = (k0 & 0x03ffffff) as u64;
        let r1 = (((k0 >> 26) | (k1 << 6)) & 0x03ffff03) as u64;
        let r2 = (((k1 >> 20) | (k2 << 12)) & 0x03ffc0ff) as u64;
        let r3 = (((k2 >> 14) | (k3 << 18)) & 0x03f03fff) as u64;
        let r4 = ((k3 >> 8) & 0x000fffff) as u64;

        let s1 = r1 * 5;
        let s2 = r2 * 5;
        let s3 = r3 * 5;
        let s4 = r4 * 5;

        let mut h0: u64 = 0;
        let mut h1: u64 = 0;
        let mut h2: u64 = 0;
        let mut h3: u64 = 0;
        let mut h4: u64 = 0;

        let mut offset = 0;
        while offset < msg.len() {
            let remaining = msg.len() - offset;
            let chunk_len = remaining.min(16);
            let mut block = [0u8; 17];
            block[..chunk_len].copy_from_slice(&msg[offset..offset + chunk_len]);
            block[chunk_len] = 0x01; // Append 1 bit (0x01 byte)

            let b0 = u32::from_le_bytes(block[0..4].try_into().unwrap());
            let b1 = u32::from_le_bytes(block[4..8].try_into().unwrap());
            let b2 = u32::from_le_bytes(block[8..12].try_into().unwrap());
            let b3 = u32::from_le_bytes(block[12..16].try_into().unwrap());
            let b4 = block[16] as u32;

            h0 += (b0 & 0x03ffffff) as u64;
            h1 += (((b0 >> 26) | (b1 << 6)) & 0x03ffffff) as u64;
            h2 += (((b1 >> 20) | (b2 << 12)) & 0x03ffffff) as u64;
            h3 += (((b2 >> 14) | (b3 << 18)) & 0x03ffffff) as u64;
            h4 += (((b3 >> 8) | (b4 << 24)) & 0x03ffffff) as u64;

            // Polynomial multiplication h * r
            let d0 = (h0 as u128) * (r0 as u128)
                + (h1 as u128) * (s4 as u128)
                + (h2 as u128) * (s3 as u128)
                + (h3 as u128) * (s2 as u128)
                + (h4 as u128) * (s1 as u128);

            let d1 = (h0 as u128) * (r1 as u128)
                + (h1 as u128) * (r0 as u128)
                + (h2 as u128) * (s4 as u128)
                + (h3 as u128) * (s3 as u128)
                + (h4 as u128) * (s2 as u128);

            let d2 = (h0 as u128) * (r2 as u128)
                + (h1 as u128) * (r1 as u128)
                + (h2 as u128) * (r0 as u128)
                + (h3 as u128) * (s4 as u128)
                + (h4 as u128) * (s3 as u128);

            let d3 = (h0 as u128) * (r3 as u128)
                + (h1 as u128) * (r2 as u128)
                + (h2 as u128) * (r1 as u128)
                + (h3 as u128) * (r0 as u128)
                + (h4 as u128) * (s4 as u128);

            let d4 = (h0 as u128) * (r4 as u128)
                + (h1 as u128) * (r3 as u128)
                + (h2 as u128) * (r2 as u128)
                + (h3 as u128) * (r1 as u128)
                + (h4 as u128) * (r0 as u128);

            // Partial reduction
            let mut c = (d0 >> 26) as u64;
            h0 = (d0 & 0x03ffffff) as u64;
            let d1 = (d1 as u64) + c;

            c = d1 >> 26;
            h1 = d1 & 0x03ffffff;
            let d2 = (d2 as u64) + c;

            c = d2 >> 26;
            h2 = d2 & 0x03ffffff;
            let d3 = (d3 as u64) + c;

            c = d3 >> 26;
            h3 = d3 & 0x03ffffff;
            let d4 = (d4 as u64) + c;

            c = d4 >> 26;
            h4 = d4 & 0x03ffffff;
            h0 += c * 5;

            c = h0 >> 26;
            h0 &= 0x03ffffff;
            h1 += c;

            offset += chunk_len;
        }

        // Full carry propagation
        let mut c = h1 >> 26;
        h1 &= 0x03ffffff;
        h2 += c;
        c = h2 >> 26;
        h2 &= 0x03ffffff;
        h3 += c;
        c = h3 >> 26;
        h3 &= 0x03ffffff;
        h4 += c;
        c = h4 >> 26;
        h4 &= 0x03ffffff;
        h0 += c * 5;
        c = h0 >> 26;
        h0 &= 0x03ffffff;
        h1 += c;

        // Compute h - p
        let mut g0 = h0.wrapping_add(5);
        c = g0 >> 26;
        g0 &= 0x03ffffff;
        let mut g1 = h1.wrapping_add(c);
        c = g1 >> 26;
        g1 &= 0x03ffffff;
        let mut g2 = h2.wrapping_add(c);
        c = g2 >> 26;
        g2 &= 0x03ffffff;
        let mut g3 = h3.wrapping_add(c);
        c = g3 >> 26;
        g3 &= 0x03ffffff;
        let g4 = h4.wrapping_add(c).wrapping_sub(1 << 26);

        // Mask to select h or h - p
        let mask = (g4 >> 63).wrapping_sub(1);
        let not_mask = !mask;

        h0 = (h0 & not_mask) | (g0 & mask);
        h1 = (h1 & not_mask) | (g1 & mask);
        h2 = (h2 & not_mask) | (g2 & mask);
        h3 = (h3 & not_mask) | (g3 & mask);
        h4 = (h4 & not_mask) | (g4 & mask);

        // Convert back to 16 bytes
        let mut h_bytes = [0u8; 16];
        let w0 = ((h0) | (h1 << 26)) as u32;
        let w1 = ((h1 >> 6) | (h2 << 20)) as u32;
        let w2 = ((h2 >> 12) | (h3 << 14)) as u32;
        let w3 = ((h3 >> 18) | (h4 << 8)) as u32;

        h_bytes[0..4].copy_from_slice(&w0.to_le_bytes());
        h_bytes[4..8].copy_from_slice(&w1.to_le_bytes());
        h_bytes[8..12].copy_from_slice(&w2.to_le_bytes());
        h_bytes[12..16].copy_from_slice(&w3.to_le_bytes());

        // Add s (key[16..32])
        let s0 = u32::from_le_bytes(key[16..20].try_into().unwrap());
        let s1 = u32::from_le_bytes(key[20..24].try_into().unwrap());
        let s2 = u32::from_le_bytes(key[24..28].try_into().unwrap());
        let s3 = u32::from_le_bytes(key[28..32].try_into().unwrap());

        let mut carry: u64 = (w0 as u64) + (s0 as u64);
        let out0 = carry as u32;
        carry = (carry >> 32) + (w1 as u64) + (s1 as u64);
        let out1 = carry as u32;
        carry = (carry >> 32) + (w2 as u64) + (s2 as u64);
        let out2 = carry as u32;
        carry = (carry >> 32) + (w3 as u64) + (s3 as u64);
        let out3 = carry as u32;

        let mut tag = [0u8; 16];
        tag[0..4].copy_from_slice(&out0.to_le_bytes());
        tag[4..8].copy_from_slice(&out1.to_le_bytes());
        tag[8..12].copy_from_slice(&out2.to_le_bytes());
        tag[12..16].copy_from_slice(&out3.to_le_bytes());
        tag
    }
}

pub struct ChaCha20Poly1305;

impl ChaCha20Poly1305 {
    pub fn encrypt(
        key: &[u8; 32],
        nonce: &[u8; 12],
        aad: &[u8],
        plaintext: &[u8],
    ) -> (Vec<u8>, [u8; 16]) {
        let chacha = ChaCha20::new(key, nonce, 0);

        // Generate Poly1305 key from block 0
        let block0 = chacha.block(0);
        let mut poly_key = [0u8; 32];
        poly_key.copy_from_slice(&block0[..32]);

        // Encrypt plaintext with counter = 1
        let ciphertext = chacha.process(1, plaintext);

        // Construct MAC input
        let mut mac_data = Vec::new();
        mac_data.extend_from_slice(aad);
        if aad.len() % 16 != 0 {
            mac_data.resize(mac_data.len() + (16 - (aad.len() % 16)), 0);
        }

        mac_data.extend_from_slice(&ciphertext);
        if ciphertext.len() % 16 != 0 {
            mac_data.resize(mac_data.len() + (16 - (ciphertext.len() % 16)), 0);
        }

        mac_data.extend_from_slice(&(aad.len() as u64).to_le_bytes());
        mac_data.extend_from_slice(&(ciphertext.len() as u64).to_le_bytes());

        let tag = Poly1305::mac(&poly_key, &mac_data);
        (ciphertext, tag)
    }

    pub fn decrypt(
        key: &[u8; 32],
        nonce: &[u8; 12],
        aad: &[u8],
        ciphertext: &[u8],
        tag: &[u8; 16],
    ) -> Result<Vec<u8>> {
        let chacha = ChaCha20::new(key, nonce, 0);

        let block0 = chacha.block(0);
        let mut poly_key = [0u8; 32];
        poly_key.copy_from_slice(&block0[..32]);

        // Construct MAC input
        let mut mac_data = Vec::new();
        mac_data.extend_from_slice(aad);
        if aad.len() % 16 != 0 {
            mac_data.resize(mac_data.len() + (16 - (aad.len() % 16)), 0);
        }

        mac_data.extend_from_slice(ciphertext);
        if ciphertext.len() % 16 != 0 {
            mac_data.resize(mac_data.len() + (16 - (ciphertext.len() % 16)), 0);
        }

        mac_data.extend_from_slice(&(aad.len() as u64).to_le_bytes());
        mac_data.extend_from_slice(&(ciphertext.len() as u64).to_le_bytes());

        let computed_tag = Poly1305::mac(&poly_key, &mac_data);

        // Constant time comparison
        let mut diff = 0u8;
        for i in 0..16 {
            diff |= computed_tag[i] ^ tag[i];
        }

        if diff != 0 {
            return Err(CraftError::Other(
                "ChaCha20Poly1305 authentication failed: cryptographic tag mismatch".to_string(),
            ));
        }

        let plaintext = chacha.process(1, ciphertext);
        Ok(plaintext)
    }
}

pub fn derive_key(passphrase: &str, salt: &[u8]) -> [u8; 32] {
    let mut current = compute_hmac_sha256(salt, passphrase.as_bytes());
    for _ in 1..10000 {
        current = compute_hmac_sha256(salt, &current);
    }
    current
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rfc8439_test_vectors() {
        // RFC 8439 Section 2.8.2 Test Vector
        let key_hex = "808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f";
        let nonce_hex = "070000004041424344454647";
        let aad_hex = "50515253c0c1c2c3c4c5c6c7";
        let plaintext = b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";

        let key: [u8; 32] = hex::decode(key_hex).unwrap().try_into().unwrap();
        let nonce: [u8; 12] = hex::decode(nonce_hex).unwrap().try_into().unwrap();
        let aad = hex::decode(aad_hex).unwrap();

        let (ciphertext, tag) = ChaCha20Poly1305::encrypt(&key, &nonce, &aad, plaintext);

        let expected_tag_hex = "1ae10b594f09e26a7e902ecbd0600691";
        assert_eq!(hex::encode(tag), expected_tag_hex);

        let decrypted = ChaCha20Poly1305::decrypt(&key, &nonce, &aad, &ciphertext, &tag).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_tamper_detection() {
        let key = [0x42u8; 32];
        let nonce = [0x19u8; 12];
        let aad = b"metadata-test";
        let plaintext = b"Sensitive server world chunk data";

        let (mut ciphertext, tag) = ChaCha20Poly1305::encrypt(&key, &nonce, aad, plaintext);

        // Tamper with ciphertext
        ciphertext[0] ^= 0x01;
        let res = ChaCha20Poly1305::decrypt(&key, &nonce, aad, &ciphertext, &tag);
        assert!(res.is_err());
    }

    #[test]
    fn test_key_derivation() {
        let key1 = derive_key("MySuperSecretPassphrase", b"craft_salt_123");
        let key2 = derive_key("MySuperSecretPassphrase", b"craft_salt_123");
        let key3 = derive_key("DifferentPassphrase", b"craft_salt_123");

        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
        assert_eq!(key1.len(), 32);
    }
}
