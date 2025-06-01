use aes::{
    cipher::{BlockEncrypt, KeyInit, generic_array::GenericArray},
    Aes128,
};

pub fn aes128_ctr(buf: &mut [u8], key: [u8; 16], iv: &[u8; 16]) {
    let cipher = Aes128::new(&key.into());
    let mut ctr = u128::from_be_bytes(*iv);

    for chunk in buf.chunks_mut(16) {
        let mut block = GenericArray::from(ctr.to_be_bytes());
        cipher.encrypt_block(&mut block);
        for (b, k) in chunk.iter_mut().zip(block.iter()) {
            *b ^= k;
        }
        ctr = ctr.wrapping_add(1);
    }
}