# Quick Start

This guide will help you get a Ream node up and running quickly.

## Prerequisites

Make sure you have [installed Ream and it's dependencies](./installation.md) before proceeding.

## Running a Lean Node

The quickest way to get started is to run a lean node on the Ephemery testnet:

```bash
cargo run --release -- --ephemeral lean_node \
    --network ephemery \
    --validator-registry-path ./bin/ream/assets/lean/validator_registry.yml
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
    --validator-registry-path ./bin/ream/assets/lean/validator_registry.yml \
    --metrics
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
