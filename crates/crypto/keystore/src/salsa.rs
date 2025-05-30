use ssz_types::{typenum::U16, FixedVector};

fn rotate(value: u32, shift: u32) -> u32 {
    (value << shift) | (value >> (32 - shift))
}

// Based on https://datatracker.ietf.org/doc/html/rfc7914#page-4
pub fn salsa20_8_core(input: &[u32; 16]) -> FixedVector<u32, U16> {
    let mut state = input.clone();
    for _ in 0..4 {
        state[4] ^= rotate(state[0].wrapping_add(state[12]), 7);
        state[8] ^= rotate(state[4].wrapping_add(state[0]), 9);
        state[12] ^= rotate(state[8].wrapping_add(state[4]),13);  
        state[0] ^= rotate(state[12].wrapping_add(state[8]),18);
        state[9] ^= rotate(state[5].wrapping_add(state[1]), 7);  
        state[13] ^= rotate(state[9].wrapping_add(state[5]), 9);
        state[1] ^= rotate(state[13].wrapping_add(state[9]),13);  
        state[5] ^= rotate(state[1].wrapping_add(state[13]),18);
        state[14] ^= rotate(state[10].wrapping_add(state[6]), 7);  
        state[2] ^= rotate(state[14].wrapping_add(state[10]), 9);
        state[6] ^= rotate(state[2].wrapping_add(state[14]),13);  
        state[10] ^= rotate(state[6].wrapping_add(state[2]),18);
        state[3] ^= rotate(state[15].wrapping_add(state[11]), 7);  
        state[7] ^= rotate(state[3].wrapping_add(state[15]), 9);
        state[11] ^= rotate(state[7].wrapping_add(state[3]),13);  
        state[15] ^= rotate(state[11].wrapping_add(state[7]),18);
        state[1] ^= rotate(state[0].wrapping_add(state[3]), 7);  
        state[2] ^= rotate(state[1].wrapping_add(state[0]), 9);
        state[3] ^= rotate(state[2].wrapping_add(state[1]),13);  
        state[0] ^= rotate(state[3].wrapping_add(state[2]),18);
        state[6] ^= rotate(state[5].wrapping_add(state[4]), 7);  
        state[7] ^= rotate(state[6].wrapping_add(state[5]), 9);
        state[4] ^= rotate(state[7].wrapping_add(state[6]),13);  
        state[5] ^= rotate(state[4].wrapping_add(state[7]),18);
        state[11] ^= rotate(state[10].wrapping_add(state[9]), 7);  
        state[8] ^= rotate(state[11].wrapping_add(state[10]), 9);
        state[9] ^= rotate(state[8].wrapping_add(state[11]),13);  
        state[10] ^= rotate(state[9].wrapping_add(state[8]),18);
        state[12] ^= rotate(state[15].wrapping_add(state[14]), 7);  
        state[13] ^= rotate(state[12].wrapping_add(state[15]), 9);
        state[14] ^= rotate(state[13].wrapping_add(state[12]),13);  
        state[15] ^= rotate(state[14].wrapping_add(state[13]),18);
    };
    FixedVector::from(
        state.iter().zip(input.iter())
            .map(|(&state_item, &input_item)| state_item.wrapping_add(input_item))
            .collect::<Vec<u32>>()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // Endian issue
    // See https://crypto.stackexchange.com/questions/95460/how-to-test-salsa20-8-core-rfc-7914-implementation-with-the-test-vectors
    // in regards to why we need to shift the test vectors
    fn shift(x: u32) -> u32 {
        (x << 24) | ((x & 0x0000ff00) << 8) | ((x & 0x00ff0000) >> 8) | ((x & 0xff000000) >> 24)
    }

    // See https://datatracker.ietf.org/doc/html/draft-josefsson-scrypt-kdf-02#page-3
    // for test vector
    #[test]
    fn test_salsa20_8_core() {
        let input = [
            0x7e879a21, 0x4f3ec986, 0x7ca940e6, 0x41718f26,
            0xbaee555b, 0x8c61c1b5, 0x0df84611, 0x6dcd3b1d,
            0xee24f319, 0xdf9b3d85, 0x14121e4b, 0x5ac5aa32,
            0x76021d29, 0x09c74829, 0xedebc68d, 0xb8b8c25e,
        ].map(shift);

        let expected_output = [
            0xa41f859c, 0x6608cc99, 0x3b81cacb, 0x020cef05,
            0x044b2181, 0xa2fd337d, 0xfd7b1c63, 0x96682f29,
            0xb4393168, 0xe3c9e6bc, 0xfe6bc5b7, 0xa06d96ba,
            0xe424cc10, 0x2c91745c, 0x24ad673d, 0xc7618f81,
        ].map(shift);

        assert_eq!(salsa20_8_core(&input), FixedVector::from(expected_output.to_vec()));
    }
}
