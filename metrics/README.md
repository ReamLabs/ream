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

## Enable metrics on lean node

Don't forget to run the lean node with metrics exporting on. Example:

```bash
cargo run --release lean_node --network ephemery --metrics
```

## View the Dashboard

View the dashboard at http://localhost:3000
