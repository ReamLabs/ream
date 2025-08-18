# Lean Metrics

You can run this docker compose file it will run

- prometheus
- grafana
- setup the default dashboard

The default username/password is `admin`/`admin`.

## Run the metrics services

Simply start docker compose in this `metrics/` folder:

```sh
docker compose up
```

If you'd like to run the metrics as part of the overall docker compose stack, you may define the parent's `compose.override.yml` file to include this metrics' `compose.yml` file in an override:

```shell
# Metrics is included in the compose.override-example.yml file
cp compose.override-example.yml compose.override.yml
```

## Enable metrics on lean node

Don't forget to run the lean node with metrics exporting on. Example:

```bash
cargo run --release lean_node --network ephemery --metrics
```


## View the Dashboard

View the dashboard at http://localhost:3000
