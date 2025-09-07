# ream

A Rust implementation of the Ethereum Beam Chain specification.

```bash
$ ream --help
```
```txt
Usage: ream [OPTIONS] <COMMAND>

Commands:
  lean_node             Start the lean node
  beacon_node           Start the beacon node
  validator_node        Start the validator node
  account_manager       Manage validator accounts
  voluntary_exit        Perform voluntary exit for a validator
  generate_private_key  Generate a secp256k1 keypair for lean node
  help                  Print this message or the help of the given subcommand(s)

Options:
      --data-dir <DATA_DIR>  The directory for storing application data. If used together with --ephemeral, new child directory will be created.
  -e, --ephemeral            Use new data directory, located in OS temporary directory. If used together with --data-dir, new directory will be created there instead.
      --purge-db             Purges the database.
  -h, --help                 Print help
  -V, --version              Print version
```
