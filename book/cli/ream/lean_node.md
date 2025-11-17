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
      --devnet <DEVNET>
          Set which devnet version to run, options are 1 and 2 [default: 1]
  -h, --help
          Print help
```
