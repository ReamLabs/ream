# Ream Account Manager

**Ream Account Manager** is a CLI tool written in **Rust** for generating **Winternitz One-Time Signature (WOTS)** validator keys for use on the **Beam Chain**.

> ⚡ Secure, deterministic, and purpose-built for lightweight cryptographic identity management.

---

## 🚀 Features

- 🔐 Generate Winternitz OTS (WOTS) key pairs
- 🧾 Export public/private keys to files or standard output
- 🏷️ Tag key generation with metadata or labels
- 🛡️ Designed with validator node integration in mind

---

## 📦 Installation

### Prerequisites
- Rust & Cargo: [Install Rust](https://www.rust-lang.org/tools/install)

### Usage

```bash
ream account_manager [OPTIONS]
```

#### Options:
- `-v, --verbosity <VERBOSITY>` - Verbosity level (default: 3)
- `-l, --lifetime <LIFETIME>` - Account lifetime in 2 ** lifetime slots (default: 28)
- `-c, --chunk-size <CHUNK_SIZE>` - Chunk size for messages (default: 5)
- `-s, --seed-phrase <SEED_PHRASE>` - Seed phrase for key generation (optional)
- `-h, --help` - Print help information

#### Examples:

Generate keys with default settings:
```bash
ream account_manager
```

Generate keys with a specific seed phrase:
```bash
ream account_manager --seed-phrase "your seed phrase here"
```

Generate keys with custom lifetime and chunk size:
```bash
ream account_manager --lifetime 20 --chunk-size 4
```
