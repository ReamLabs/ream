# ream validator_node



```bash
$ ream validator_node --help
```
```txt
Usage: ream validator_node [OPTIONS] --import-keystores <IMPORT_KEYSTORES> --suggested-fee-recipient <SUGGESTED_FEE_RECIPIENT>

Options:
  -v, --verbosity <VERBOSITY>
          Verbosity level [default: 3]
      --beacon-api-endpoint <BEACON_API_ENDPOINT>
          Set HTTP url of the beacon api endpoint [default: http://localhost:5052]
      --request-timeout <REQUEST_TIMEOUT>
          Set HTTP request timeout for beacon api calls [default: 60]
      --key-manager-http-address <KEY_MANAGER_HTTP_ADDRESS>
          Set HTTP address of the key manager server [default: 127.0.0.1]
      --key-manager-http-port <KEY_MANAGER_HTTP_PORT>
          Set HTTP Port of the key manager server [default: 8008]
      --network <NETWORK>
          Choose mainnet, holesky, sepolia, hoodi, dev or provide a path to a YAML config file [default: mainnet]
      --import-keystores <IMPORT_KEYSTORES>
          The directory for importing keystores
      --suggested-fee-recipient <SUGGESTED_FEE_RECIPIENT>
          The suggested fee recipient address where staking rewards would go to
      --password-file <PASSWORD_FILE>
          The plaintext password file to use for keystores
      --password <PASSWORD>
          The password to use for keystores. It's recommended to use password-file over this in order to prevent your keystore password from appearing in the shell history
  -h, --help
          Print help
```
