# ream voluntary_exit

Perform voluntary exit for a validator

```bash
$ ream voluntary_exit --help
```
```txt
Usage: ream voluntary_exit [OPTIONS] --import-keystores <IMPORT_KEYSTORES> --validator-index <VALIDATOR_INDEX> --epoch <EPOCH>

Options:
  -v, --verbosity <VERBOSITY>
          Verbosity level [default: 3]
      --beacon-api-endpoint <BEACON_API_ENDPOINT>
          Set HTTP url of the beacon api endpoint [default: http://localhost:5052]
      --request-timeout <REQUEST_TIMEOUT>
          Set HTTP request timeout for beacon api calls [default: 60]
      --network <NETWORK>
          Choose mainnet, holesky, sepolia, hoodi, dev or provide a path to a YAML config file [default: mainnet]
      --import-keystores <IMPORT_KEYSTORES>
          The directory for importing keystores
      --password-file <PASSWORD_FILE>
          The plaintext password file to use for keystores
      --password <PASSWORD>
          The password to use for keystores. It's recommended to use password-file over this in order to prevent your keystore password from appearing in the shell history
      --validator-index <VALIDATOR_INDEX>
          The validator index to exit
      --epoch <EPOCH>
          The epoch for the voluntary exit
      --wait
          Wait until the validator has fully exited
  -h, --help
          Print help
```
