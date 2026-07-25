# ream lean_node

Start the lean node

```bash
$ ream lean_node --help
```
```txt
Usage: ream lean_node [OPTIONS] --network <NETWORK> --validator-registry-path <VALIDATOR_REGISTRY_PATH>

Options:
      --network <NETWORK>
          Provide a path to a YAML config file, or use 'ephemery' for the Ephemery network
      --bootnodes <BOOTNODES>
          Bootnodes configuration: Use 'default' for network defaults, 'none' to disable, '/path/to/nodes.yaml' for a YAML file with ENRs, or comma-delimited base64-encoded ENRs [default: default]
      --checkpoint-sync-url <CHECKPOINT_SYNC_URL>
          HTTP URL of a remote node to sync checkpoint state from
      --validator-registry-path <VALIDATOR_REGISTRY_PATH>
          The path to the validator registry
      --node-id <NODE_ID>
          Node identifier for validator registry (e.g., 'ream_0', 'zeam_0') [default: ream_0]
      --private-key-path <PRIVATE_KEY_PATH>
          The path to the hex encoded secp256k1 libp2p key
      --socket-address <SOCKET_ADDRESS>
          Set P2P socket address [default: 0.0.0.0]
      --socket-port <SOCKET_PORT>
          Set P2P socket port (QUIC) [default: 9000]
      --http-address <HTTP_ADDRESS>
          Set HTTP address [default: 127.0.0.1]
      --http-port <HTTP_PORT>
          Set HTTP Port [default: 5052]
      --http-allow-origin

      --metrics
          Enable metrics
      --metrics-address <METRICS_ADDRESS>
          Set metrics address [default: 127.0.0.1]
      --metrics-port <METRICS_PORT>
          Set metrics port [default: 8080]
      --is-aggregator
          Set node as aggregator for committee signature aggregation
      --aggregate-subnet-ids <AGGREGATE_SUBNET_IDS>
          Additional attestation subnet ids to subscribe to and aggregate from (comma-separated, e.g. '0,3,7'). Requires --is-aggregator.
      --attestation-committee-count <ATTESTATION_COMMITTEE_COUNT>
          Number of attestation committees (subnets). Each validator's subnet is `validator_id % count`. [default: 1]
      --reth-datadir <RETH_DATADIR>
          Set reth data directory (needs `--features reth`) [default: ./reth-data]
      --reth-rpc-address <RETH_RPC_ADDRESS>
          Set reth eth_* JSON-RPC address [default: 127.0.0.1]
      --reth-rpc-port <RETH_RPC_PORT>
          Set reth eth_* JSON-RPC port [default: 8545]
      --reth-p2p-address <RETH_P2P_ADDRESS>
          Set reth RLPx (devp2p) address [default: 127.0.0.1]
      --reth-p2p-port <RETH_P2P_PORT>
          Set reth RLPx (devp2p) port, to gossip transactions with other execution layers. Unset means an isolated EL with no peers
      --reth-p2p-secret <RETH_P2P_SECRET>
          Set 32-byte hex secp256k1 key pinning reth's enode identity, so peers can address it deterministically
      --reth-trusted-peers <RETH_TRUSTED_PEERS>
          Comma-separated enode URLs of other execution layers to dial directly as a static trusted mesh (enode://<pubkey>@<ip>:<port>). Discovery stays off.
  -h, --help
          Print help
```
