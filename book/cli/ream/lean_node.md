# ream lean_node

Start the lean node

```bash
$ ream lean_node --help
```
```txt
Usage: ream lean_node [OPTIONS] --network <NETWORK>

Options:
  -v, --verbosity <VERBOSITY>
          Verbosity level [default: 3]
      --network <NETWORK>
          Provide a path to a YAML config file, or use 'ephemery' for the Ephemery network
      --bootnodes <BOOTNODES>
          One or more comma-delimited base64-encoded ENR's of peers to initially connect to. Use 'default' to use the default bootnodes for the network. Use 'none' to disable bootnodes. [default: default]
      --socket-address <SOCKET_ADDRESS>
          Set P2P socket address [default: 0.0.0.0]
      --socket-port <SOCKET_PORT>
          Set P2P socket port (TCP) [default: 9000]
      --http-address <HTTP_ADDRESS>
          Set HTTP address [default: 127.0.0.1]
      --http-port <HTTP_PORT>
          Set HTTP Port [default: 5052]
      --http-allow-origin

  -h, --help
          Print help
```
