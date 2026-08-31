//! `ream-data-availability-tool` — a small end-to-end driver for the standalone data node.
//! Usage:
//!   ream-data-availability-tool health
//!   ream-data-availability-tool availability <block_root>
//!   ream-data-availability-tool feed --beacon-url <url> [block_id] [--columns 0,1,2] [--wait 30] [--per-column]
//!   ream-data-availability-tool generate [--slot 1] [--blobs 1] [--out dir] [--per-column]

mod beacon;
mod data_availability;
mod generate;

use std::path::{Path, PathBuf};

use alloy_primitives::B256;
use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use ream_consensus_beacon::data_column_sidecar::DataColumnSidecar;
use ssz::Encode;

use crate::{
    beacon::build_column_sidecars, data_availability::DataAvailabilityClient,
    generate::build_synthetic_sidecars,
};

#[derive(Parser)]
#[command(
    name = "ream-data-availability-tool",
    about = "End-to-end driver for the standalone ream data node"
)]
struct Cli {
    /// Base URL of the data node RPC.
    #[arg(long, default_value = "http://127.0.0.1:5062")]
    da_url: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check that the data node is up.
    Health,
    /// Query column availability for a block root.
    Availability { block_root: B256 },
    /// Fetch a block's blobs from a beacon node, derive its data column
    /// sidecars, submit them to the data node, and wait for them to be held.
    Feed {
        /// Beacon API base URL (e.g. http://localhost:5052 or a public
        /// provider).
        #[arg(long)]
        beacon_url: String,
        /// Block to feed: a slot number, "head", "finalized", or a 0x block
        /// root.
        #[arg(default_value = "head")]
        block_id: String,
        /// Submit only these column indices (comma-separated). Default: all.
        #[arg(long, value_delimiter = ',')]
        columns: Option<Vec<u64>>,
        /// Seconds to wait for the data node to verify the submitted columns.
        #[arg(long, default_value_t = 30)]
        wait: u64,
        /// Submit one JSON request per column through `/ingest`
        #[arg(long)]
        per_column: bool,
    },
    /// Synthesize a block's worth of KZG-valid sidecars offline — no beacon
    /// node needed — and submit them, or write them as ingest-ready vectors.
    Generate {
        /// Header slot of the synthetic block (the block root varies with it).
        #[arg(long, default_value_t = 1)]
        slot: u64,
        /// All-zero blobs in the synthetic block. Must stay within the blob
        /// limit at `slot`'s epoch or the data node will reject the columns.
        #[arg(long, default_value_t = 1)]
        blobs: usize,
        /// Write ingest-ready JSON request bodies to this directory instead of
        /// submitting them.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Submit only these column indices (comma-separated). Default: all.
        #[arg(long, value_delimiter = ',')]
        columns: Option<Vec<u64>>,
        /// Seconds to wait for the data node to verify the submitted columns.
        #[arg(long, default_value_t = 30)]
        wait: u64,
        /// Submit one JSON request per column through `/ingest`
        #[arg(long)]
        per_column: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = DataAvailabilityClient::new(cli.da_url);

    match cli.command {
        Command::Health => {
            let health = client.health().await?;
            println!("{}", serde_json::to_string_pretty(&health)?);
        }
        Command::Availability { block_root } => {
            let availability = client.availability(block_root).await?;
            println!("{}", serde_json::to_string_pretty(&availability)?);
        }
        Command::Feed {
            beacon_url,
            block_id,
            columns,
            wait,
            per_column,
        } => {
            let blob_sidecars = beacon::fetch_blob_sidecars(&beacon_url, &block_id)
                .await
                .with_context(|| format!("fetching blob sidecars for block {block_id}"))?;
            if blob_sidecars.is_empty() {
                bail!("block {block_id} has no blobs — nothing to feed");
            }
            println!(
                "fetched {} blob(s) for block {block_id}, slot {}",
                blob_sidecars.len(),
                blob_sidecars[0].signed_block_header.message.slot
            );

            // KZG cell computation is CPU-bound and takes a while (trusted
            // setup load plus per-blob proving); keep it off the async runtime.
            let (block_root, slot, sidecars) =
                tokio::task::spawn_blocking(move || build_column_sidecars(blob_sidecars)).await??;
            println!(
                "derived {} column sidecars (block root {block_root})",
                sidecars.len()
            );

            let selected = select_columns(sidecars, columns.as_deref())?;
            submit_and_wait(&client, block_root, slot, &selected, wait, per_column).await?;
        }
        Command::Generate {
            slot,
            blobs,
            out,
            columns,
            wait,
            per_column,
        } => {
            let (block_root, slot, sidecars) =
                tokio::task::spawn_blocking(move || build_synthetic_sidecars(slot, blobs))
                    .await??;
            println!(
                "generated {} column sidecars from {blobs} synthetic blob(s) \
                 (block root {block_root})",
                sidecars.len()
            );

            let selected = select_columns(sidecars, columns.as_deref())?;
            match out {
                Some(dir) => write_vectors(&dir, block_root, slot, &selected)?,
                None => {
                    submit_and_wait(&client, block_root, slot, &selected, wait, per_column).await?
                }
            }
        }
    }

    Ok(())
}

/// Keep only the requested column indices (or everything when none given).
fn select_columns(
    sidecars: Vec<DataColumnSidecar>,
    columns: Option<&[u64]>,
) -> Result<Vec<DataColumnSidecar>> {
    let selected: Vec<_> = match columns {
        Some(indices) => sidecars
            .into_iter()
            .filter(|sidecar| indices.contains(&sidecar.index))
            .collect(),
        None => sidecars,
    };
    if selected.is_empty() {
        bail!("no sidecars match the requested column indices");
    }
    Ok(selected)
}

/// The hex-encoded SSZ payload the ingest endpoint expects.
fn payload_hex(sidecar: &DataColumnSidecar) -> String {
    format!(
        "0x{}",
        alloy_primitives::hex::encode(sidecar.as_ssz_bytes())
    )
}

/// Submit the sidecars — one SSZ block batch by default, or one JSON request
/// per column with `--per-column`
async fn submit_and_wait(
    client: &DataAvailabilityClient,
    block_root: B256,
    slot: u64,
    sidecars: &[DataColumnSidecar],
    wait_secs: u64,
    per_column: bool,
) -> Result<()> {
    let submitted: Vec<u64> = sidecars.iter().map(|sidecar| sidecar.index).collect();
    let started = std::time::Instant::now();

    if per_column {
        for sidecar in sidecars {
            client
                .ingest(block_root, sidecar.index, slot, &payload_hex(sidecar))
                .await
                .with_context(|| format!("submitting column {}", sidecar.index))?;
        }
        println!(
            "submitted {} column(s) in {} JSON request(s) — {:.2?}",
            submitted.len(),
            submitted.len(),
            started.elapsed()
        );
    } else {
        let columns: Vec<(u64, Vec<u8>)> = sidecars
            .iter()
            .map(|sidecar| (sidecar.index, sidecar.as_ssz_bytes()))
            .collect();
        client
            .ingest_block(block_root, slot, &columns)
            .await
            .context("submitting the block batch")?;
        println!(
            "submitted {} column(s) in one SSZ batch request — {:.2?}",
            submitted.len(),
            started.elapsed()
        );
    }

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(wait_secs);
    let availability = loop {
        let availability = client.availability(block_root).await?;
        let missing: Vec<u64> = availability["missing"]
            .as_array()
            .map(|values| values.iter().filter_map(|v| v.as_u64()).collect())
            .unwrap_or_default();
        if submitted.iter().all(|index| !missing.contains(index)) {
            break availability;
        }
        if tokio::time::Instant::now() >= deadline {
            println!("{}", serde_json::to_string_pretty(&availability)?);
            bail!(
                "timed out after {wait_secs}s: {} submitted column(s) still not held",
                submitted
                    .iter()
                    .filter(|index| missing.contains(index))
                    .count()
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    };

    println!(
        "all submitted columns verified and held — {:.2?} end to end (±100ms poll granularity):",
        started.elapsed()
    );
    println!("{}", serde_json::to_string_pretty(&availability)?);
    Ok(())
}

/// Write one ingest-ready JSON request body per column, for driving the data
/// node with nothing but `curl`.
fn write_vectors(
    dir: &Path,
    block_root: B256,
    slot: u64,
    sidecars: &[DataColumnSidecar],
) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    for sidecar in sidecars {
        let body = serde_json::json!({
            "block_root": block_root,
            "index": sidecar.index,
            "slot": slot,
            "payload": payload_hex(sidecar),
        });
        std::fs::write(
            dir.join(format!("column_{}.json", sidecar.index)),
            serde_json::to_string(&body)?,
        )?;
    }
    println!(
        "wrote {} ingest-ready vector(s) to {} (block root: {block_root})",
        sidecars.len(),
        dir.display()
    );
    println!(
        "submit one with: curl -X POST <da-url>/data/v0/ingest \
         -H 'content-type: application/json' -d @{}/column_0.json",
        dir.display()
    );
    Ok(())
}
