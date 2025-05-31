use crate::encrypted_keystore::{EncryptedKeystore, KdfParams};
use crate::pbkdf2::pbkdf2;
use crate::scrypt::scrypt;
use anyhow;
use sha2::{Digest, Sha256};

impl EncryptedKeystore {
    pub fn validate_password(&self, password: &[u8]) -> anyhow::Result<bool> {
        let derived_key = match &self.crypto.kdf.params {
            KdfParams::Pbkdf2 { c, dklen, prf: _, salt } => {
                pbkdf2(password, salt, *c, *dklen)?
            }
            KdfParams::Scrypt { n, p, r, dklen, salt } => {
                scrypt(password, salt, *n, *p, *r, *dklen)?
            }
        };
        let derived_key_slice = &derived_key[16..32];
        let pre_image = [derived_key_slice, &self.crypto.cipher.message].concat();
        let checksum = Sha256::digest(&pre_image);
        let valid_password = checksum.as_slice() == self.crypto.checksum.message.as_slice();
        Ok(valid_password)
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::hex;

    use crate::encrypted_keystore::EncryptedKeystore;

    #[test]
    fn password_validation_pbkdf2() {
        let keystore = EncryptedKeystore::load_from_file("./assets/Pbkdf2TestKeystore.json").unwrap();
        let password = hex!("7465737470617373776f7264f09f9491");

        assert_eq!(true,keystore.validate_password(&password).unwrap());
    }

    #[test]
    fn password_validation_scrypt() {
        let keystore = EncryptedKeystore::load_from_file("./assets/ScryptKeystore.json").unwrap();
        let password = b"password123";

        assert_eq!(true,keystore.validate_password(password).unwrap());
    }

    #[test]
    fn password_validation_pbkdf2_invalid() {
        let keystore = EncryptedKeystore::load_from_file("./assets/Pbkdf2TestKeystore.json").unwrap();
        let password = b"password123";

        assert_eq!(false,keystore.validate_password(password).unwrap());
    }
}