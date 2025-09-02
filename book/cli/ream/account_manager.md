# ream account_manager

Manage validator accounts

```bash
$ ream account_manager --help
```
```txt
Usage: ream account_manager [OPTIONS]

Options:
  -v, --verbosity <VERBOSITY>                  Verbosity level [default: 3]
  -l, --lifetime <LIFETIME>                    Account lifetime in 2 ** lifetime slots [default: 18]
  -c, --chunk-size <CHUNK_SIZE>                Chunk size for messages [default: 5]
  -s, --seed-phrase <SEED_PHRASE>              Seed phrase for key generation
      --passphrase <PASSPHRASE>                Optional BIP39 passphrase used with the seed phrase
      --activation-epoch <ACTIVATION_EPOCH>    Activation epoch for the validator [default: 0]
      --num-active-epochs <NUM_ACTIVE_EPOCHS>  Number of active epochs [default: 262144]
      --path <PATH>                            Path for keystore file [default: ./.keystore/]
      --import-keystore <IMPORT_KEYSTORE>      Import existing keystore
  -h, --help                                   Print help
```
