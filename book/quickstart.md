# Quick Start

This guide will help you get a Ream node up and running quickly.

## Build from Source

You can build Ream on Linux.

### Dependencies

First install Rust using <a href="https://rustup.rs/">rustup</a>:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

There are some other dependencies you need to install based on your operating system (OS):

- **Ubuntu/Debian**: `apt-get install libclang-dev pkg-config libssl-dev build-essential`

Install cargo-sort and cargo-udeps tools.

```bash
cargo install cargo-udeps --locked
cargo install --git https://github.com/DevinR528/cargo-sort.git --rev 25a60ad860ce7cd0055abf4b69c18285cb07ab41 cargo-sort
```

### Build Ream

Clone the repository and move to the directory:

```bash
git clone git@github.com:reamlabs/ream.git
cd ream
```

After everything is setup, you can start the build:

```bash
make build
```

## Running a Lean Node

The quickest way to get started is to run a lean node on the Ephemery testnet:

```bash
cargo run --release -- --ephemeral lean_node \
    --network ephemery \
    --validator-registry-path ./bin/ream/assets/lean/validator_registry.yaml
```

Understanding the Command

- cargo run --release - Builds and runs Ream in release mode
- --ephemeral - Run in ephemeral mode (data is not persisted)
- lean_node - Start a lean consensus node
- --network ephemery - Use the Ephemery network
- --validator-registry-path - Path to the validator registry configuration


## Metrics

To enable your node to expose metrics through Prometheus, add the `--metrics` flag:

```bash
cargo run --release -- --ephemeral lean_node \
    --network ephemery \
    --validator-registry-path ./bin/ream/assets/lean/validators.yaml \
    --metrics --metrics-address 0.0.0.0
```

By default, metrics are exposed on `127.0.0.1:8080`. 

For a complete list of all commands and flags for running a lean node, see the [`ream lean_node` CLI 
Reference](./cli/ream/lean_node.md).

## Visualizing Metrics with Grafana

The repository includes a pre-configured Prometheus and Grafana setup in the metrics/ directory. To run the metrics
stack:

```bash
cd metrics
docker compose up
```

This will start:
- Prometheus (scrapes metrics from your node)
- Grafana (visualizes metrics with a pre-configured dashboard)

View the dashboard at http://localhost:3000 and use the default credentials: `admin/admin`.

## Running a Local PQ Devnet

For local development and testing, you can run a local PQ devnet [here](https://github.com/ReamLabs/local-pq-devnet).
