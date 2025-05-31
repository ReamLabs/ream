#[inline(always)]
fn rotate(value: u32, shift: u32) -> u32 {
    (value << shift) | (value >> (32 - shift))
}

/// Based on https://datatracker.ietf.org/doc/html/rfc7914#page-4
pub fn salsa20_8_core(byte_stream: &mut [u8; 64]) {
    let mut state = [0u32; 16];
    let mut original = [0u32; 16];

    for i in 0..16 {
        let word = {
            let mut last_four_bytes_array = [0u8; 4];
            last_four_bytes_array.copy_from_slice(&byte_stream[i * 4..i * 4 + 4]);
            u32::from_le_bytes(last_four_bytes_array)
        };
        state[i] = word;
        original[i] = word;
    }

    for _ in 0..4 {
        state[4] ^= rotate(state[0].wrapping_add(state[12]), 7);
        state[8] ^= rotate(state[4].wrapping_add(state[0]), 9);
        state[12] ^= rotate(state[8].wrapping_add(state[4]), 13);
        state[0] ^= rotate(state[12].wrapping_add(state[8]), 18);
        state[9] ^= rotate(state[5].wrapping_add(state[1]), 7);
        state[13] ^= rotate(state[9].wrapping_add(state[5]), 9);
        state[1] ^= rotate(state[13].wrapping_add(state[9]), 13);
        state[5] ^= rotate(state[1].wrapping_add(state[13]), 18);
        state[14] ^= rotate(state[10].wrapping_add(state[6]), 7);
        state[2] ^= rotate(state[14].wrapping_add(state[10]), 9);
        state[6] ^= rotate(state[2].wrapping_add(state[14]), 13);
        state[10] ^= rotate(state[6].wrapping_add(state[2]), 18);
        state[3] ^= rotate(state[15].wrapping_add(state[11]), 7);
        state[7] ^= rotate(state[3].wrapping_add(state[15]), 9);
        state[11] ^= rotate(state[7].wrapping_add(state[3]), 13);
        state[15] ^= rotate(state[11].wrapping_add(state[7]), 18);
        state[1] ^= rotate(state[0].wrapping_add(state[3]), 7);
        state[2] ^= rotate(state[1].wrapping_add(state[0]), 9);
        state[3] ^= rotate(state[2].wrapping_add(state[1]), 13);
        state[0] ^= rotate(state[3].wrapping_add(state[2]), 18);
        state[6] ^= rotate(state[5].wrapping_add(state[4]), 7);
        state[7] ^= rotate(state[6].wrapping_add(state[5]), 9);
        state[4] ^= rotate(state[7].wrapping_add(state[6]), 13);
        state[5] ^= rotate(state[4].wrapping_add(state[7]), 18);
        state[11] ^= rotate(state[10].wrapping_add(state[9]), 7);
        state[8] ^= rotate(state[11].wrapping_add(state[10]), 9);
        state[9] ^= rotate(state[8].wrapping_add(state[11]), 13);
        state[10] ^= rotate(state[9].wrapping_add(state[8]), 18);
        state[12] ^= rotate(state[15].wrapping_add(state[14]), 7);
        state[13] ^= rotate(state[12].wrapping_add(state[15]), 9);
        state[14] ^= rotate(state[13].wrapping_add(state[12]), 13);
        state[15] ^= rotate(state[14].wrapping_add(state[13]), 18);
    }

    for i in 0..16 {
        byte_stream[i * 4..(i + 1) * 4]
            .copy_from_slice(&(state[i].wrapping_add(original[i])).to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // See https://datatracker.ietf.org/doc/html/draft-josefsson-scrypt-kdf-02#page-3
    // for test vector
    #[test]
    fn test_salsa20_8_core() {
        let mut stream = [
            0x7e, 0x87, 0x9a, 0x21, 0x4f, 0x3e, 0xc9, 0x86, 0x7c, 0xa9, 0x40, 0xe6, 0x41, 0x71,
            0x8f, 0x26, 0xba, 0xee, 0x55, 0x5b, 0x8c, 0x61, 0xc1, 0xb5, 0x0d, 0xf8, 0x46, 0x11,
            0x6d, 0xcd, 0x3b, 0x1d, 0xee, 0x24, 0xf3, 0x19, 0xdf, 0x9b, 0x3d, 0x85, 0x14, 0x12,
            0x1e, 0x4b, 0x5a, 0xc5, 0xaa, 0x32, 0x76, 0x02, 0x1d, 0x29, 0x09, 0xc7, 0x48, 0x29,
            0xed, 0xeb, 0xc6, 0x8d, 0xb8, 0xb8, 0xc2, 0x5e,
        ];

        let expected_output = [
            0xa4, 0x1f, 0x85, 0x9c, 0x66, 0x08, 0xcc, 0x99, 0x3b, 0x81, 0xca, 0xcb, 0x02, 0x0c,
            0xef, 0x05, 0x04, 0x4b, 0x21, 0x81, 0xa2, 0xfd, 0x33, 0x7d, 0xfd, 0x7b, 0x1c, 0x63,
            0x96, 0x68, 0x2f, 0x29, 0xb4, 0x39, 0x31, 0x68, 0xe3, 0xc9, 0xe6, 0xbc, 0xfe, 0x6b,
            0xc5, 0xb7, 0xa0, 0x6d, 0x96, 0xba, 0xe4, 0x24, 0xcc, 0x10, 0x2c, 0x91, 0x74, 0x5c,
            0x24, 0xad, 0x67, 0x3d, 0xc7, 0x61, 0x8f, 0x81,
        ];

        salsa20_8_core(&mut stream);
        assert_eq!(stream, expected_output);
    }
}
