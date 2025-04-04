# Build from Source

You can build Ream on Linux.

## Dependencies

First install Rust using <a href="https://rustup.rs/">rustup</a>:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

There are some other dependencies you need to install based on your operating system (OS):

- **Ubuntu/Debian**: `apt-get install libclang-dev pkg-config libssl-dev build-essential`

Install cargo-sort and cargo-udeps tools.

```bash
cargo install cargo-sort cargo-udeps --locked
```

## Build Ream

Clone the repository and move to the directory:

```bash
git clone git@github.com:reamlabs/ream.git
cd ream
```

After everything is setup, you can start the build:

```bash
make build
```