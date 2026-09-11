#![allow(dead_code)]

use std::convert::TryInto;

pub struct Sha256 {
    state: [u32; 8],
    completed_data_blocks: u64,
    pending: [u8; 64],
    num_pending: usize,
}

impl Default for Sha256 {
    fn default() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
                0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
            ],
            completed_data_blocks: 0,
            pending: [0u8; 64],
            num_pending: 0,
        }
    }
}

impl Sha256 {
    fn update_state(state: &mut [u32; 8], data: &[u8; 64]) {
        let mut w = [0u32; 64];

        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                data[i * 4], data[i * 4 + 1], data[i * 4 + 2], data[i * 4 + 3],
            ]);
        }

        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }

        let mut a = state[0];
        let mut b = state[1];
        let mut c = state[2];
        let mut d = state[3];
        let mut e = state[4];
        let mut f = state[5];
        let mut g = state[6];
        let mut h = state[7];

        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
            0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
            0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
            0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
            0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
            0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
            0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
        ];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
        state[5] = state[5].wrapping_add(f);
        state[6] = state[6].wrapping_add(g);
        state[7] = state[7].wrapping_add(h);
    }

    pub fn update(&mut self, data: &[u8]) {
        let mut data_offset = 0;
        let mut data_len = data.len();

        if self.num_pending > 0 && self.num_pending + data_len >= 64 {
            let copy_len = 64 - self.num_pending;
            self.pending[self.num_pending..self.num_pending + copy_len].copy_from_slice(&data[..copy_len]);
            Self::update_state(&mut self.state, &self.pending);
            self.completed_data_blocks += 1;
            data_offset = copy_len;
            data_len -= copy_len;
            self.num_pending = 0;
        }

        let full_blocks = data_len / 64;
        for i in 0..full_blocks {
            let start = data_offset + i * 64;
            let block: [u8; 64] = data[start..start + 64].try_into().unwrap();
            Self::update_state(&mut self.state, &block);
        }
        self.completed_data_blocks += full_blocks as u64;

        let remaining = data_len % 64;
        if remaining > 0 {
            let start = data_offset + full_blocks * 64;
            self.pending[self.num_pending..self.num_pending + remaining].copy_from_slice(&data[start..start + remaining]);
            self.num_pending += remaining;
        }
    }

    pub fn finish(mut self) -> [u8; 32] {
        let total_bits = self.completed_data_blocks * 512 + self.num_pending as u64 * 8;
        let mut padding = [0u8; 128];
        padding[0] = 0x80;

        let padding_len = if self.num_pending < 56 {
            56 - self.num_pending
        } else {
            120 - self.num_pending
        };

        padding[padding_len..padding_len + 8].copy_from_slice(&total_bits.to_be_bytes());
        self.update(&padding[..padding_len + 8]);

        let mut result = [0u8; 32];
        for i in 0..8 {
            let bytes = self.state[i].to_be_bytes();
            result[i * 4..(i + 1) * 4].copy_from_slice(&bytes);
        }
        result
    }

    pub fn digest(data: &[u8]) -> [u8; 32] {
        let mut hasher = Sha256::default();
        hasher.update(data);
        hasher.finish()
    }
}