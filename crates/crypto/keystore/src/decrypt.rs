use aes::{
    Aes128,
    cipher::{BlockEncrypt, KeyInit, generic_array::GenericArray},
};

pub fn aes128_ctr(buffer: &mut [u8], key: [u8; 16], initial_vector: &[u8; 16]) {
    let cipher = Aes128::new(&key.into());
    let mut counter = u128::from_be_bytes(*initial_vector);

    for chunk in buffer.chunks_mut(16) {
        let mut block = GenericArray::from(counter.to_be_bytes());
        cipher.encrypt_block(&mut block);
        for (chunk_byte, block_byte) in chunk.iter_mut().zip(block.iter()) {
            *chunk_byte ^= block_byte;
        }
        counter = counter.wrapping_add(1);
    }
}
