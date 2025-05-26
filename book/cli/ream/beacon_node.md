# ream beacon_node

Start the node

```bash
$ ream beacon_node --help
```
```txt
Usage: ream beacon_node [OPTIONS]

Options:
  -v, --verbosity <VERBOSITY>
          Verbosity level [default: 3]
      --network <NETWORK>
          Choose mainnet, holesky, sepolia, hoodi, dev or provide a path to a YAML config file [default: mainnet]
      --http-address <HTTP_ADDRESS>
          Set HTTP address [default: 127.0.0.1]
      --http-port <HTTP_PORT>
          Set HTTP Port [default: 5052]
      --http-allow-origin

      --socket-address <SOCKET_ADDRESS>
          Set P2P socket address [default: 0.0.0.0]
      --socket-port <SOCKET_PORT>
          Set P2P socket port (TCP) [default: 9000]
      --discovery-port <DISCOVERY_PORT>
          Discovery 5 listening port (UDP) [default: 9000]
      --disable-discovery
          Disable Discv5
      --data-dir <DATA_DIR>
          The directory for storing application data. If used together with --ephemeral, new child directory will be created.
  -e, --ephemeral
          Use new data directory, located in OS temporary directory. If used together with --data-dir, new directory will be created there instead.
      --bootnodes <BOOTNODES>
          One or more comma-delimited base64-encoded ENR's of peers to initially connect to. Use 'default' to use the default bootnodes for the network. Use 'none' to disable bootnodes. [default: default]
      --checkpoint-sync-url <CHECKPOINT_SYNC_URL>
          Trusted RPC URL to initiate Checkpoint Sync.
      --purge-db
          Purges the database.
      --execution-endpoint <EXECUTION_ENDPOINT>
          The URL of the execution endpoint. This is used to send requests to the engine api.
      --execution-jwt-secret <EXECUTION_JWT_SECRET>
          The JWT secret used to authenticate with the execution endpoint. This is used to send requests to the engine api.
  -h, --help
          Print help
```
