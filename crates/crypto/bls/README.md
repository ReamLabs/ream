# Ream BLS Cryptography

This crate provides BLS (Boneh-Lynn-Shacham) signature functionality with support for two different backend implementations:

- [zkcrypto/bls12_381](https://github.com/zkcrypto/bls12_381) - Optimized for zkVMs, used by **default**
- [supranational/blst](https://github.com/supranational/blst) - Optimized for performance

## Features

- ✅ BLS signature verification
- ✅ Public key aggregation
- ✅ Fast aggregate verification
- ✅ Serialization/deserialization support
- ✅ SSZ encoding/decoding
- ✅ Tree hashing support
- ❌ Secret key
- ❌ Signing
- ❌ Signature aggregation

## Usage

Add the crate to your `Cargo.toml`:

```toml
[dependencies]
ream-bls = { workspace = true, features = ["supranational"] } # For blst backend
OR
ream-bls = { workspace = true, features = ["zkcrypto"] } # For bls12_381 backend
```

## Rationale

This crate supports two different BLS implementations to serve different use cases:

- The `zkcrypto/bls12_381` backend (default) is optimized for zkVMs and can be compiled to RISC-V, making it suitable for zero-knowledge proof applications and our zkVM proof-of-concept work.

- The `supranational/blst` backend provides high-performance BLS operations, though it currently has limited platform support(RISC-V).

The crate uses a **trait-based interface** to abstract over the specific backend implementation, allowing users to switch between backends by simply changing the feature flag. This modular design also makes it easier to add support for additional BLS implementations in the future if needed.

## Example

### Public Key Aggregation

```rust
use ream_bls::{AggregatePubKey, PubKey};

let pubkeys: &[&PubKey] = &[..];
let agg_pubkey = AggregatePubKey::aggregate(pubkeys).unwrap();
```

### Signature Verification

```rust
use ream_bls::{BLSSignature, PubKey};

let signature: BLSSignature = ..;
let pubkey: PubKey = ..;
let message = b"Hello, world!";

let result = signature.verify(&pubkey, message);
```

### Fast Aggregate Verification

```rust
use ream_bls::{AggregatePubKey, BLSSignature, PubKey};

let signature: BLSSignature = ..;
let pubkeys: &[&PubKey] = &[..];
let message = b"Hello, world!";

let result = signature.fast_aggregate_verify(pubkeys, message);
```
