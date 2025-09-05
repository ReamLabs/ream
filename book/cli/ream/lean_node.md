# ream lean_node

Start the lean node

```bash
$ ream lean_node --help
```
```txt
Usage: ream lean_node [OPTIONS] --network <NETWORK> --validator-registry-path <VALIDATOR_REGISTRY_PATH>

Options:
  -v, --verbosity <VERBOSITY>
          Verbosity level [default: 3]
      --network <NETWORK>
          Provide a path to a YAML config file, or use 'ephemery' for the Ephemery network
      --bootnodes <BOOTNODES>
          One or more comma-delimited base64-encoded ENR's of peers to initially connect to. Use 'default' to use the default bootnodes for the network. Use 'none' to disable bootnodes. [default: default]
      --validator-registry-path <VALIDATOR_REGISTRY_PATH>
          The path to the validator registry
      --secret-key-path <SECRET_KEY_PATH>
          The path to the protobuf encoded secp256k1 libp2p key
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
  -h, --help
          Print help
```
