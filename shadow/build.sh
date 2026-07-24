#!/usr/bin/env bash
#
# A Cargo `[patch.crates-io]` table cannot be gated behind a feature flag, so the
# quinn-udp fallback patch is kept out of the committed Cargo.toml. This script
# temporarily appends it (from shadow/cargo-patch.toml), runs the given command,
# then restores Cargo.toml and Cargo.lock to their pristine state.
#
# Usage:
#   shadow/build.sh cargo build --release \
#       --no-default-features --features shadow-integration --bin ream
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [ "$#" -eq 0 ]; then
    echo "usage: $0 <cargo command...>" >&2
    exit 64
fi

# Back up the manifest + lockfile so we can restore them verbatim, regardless of
# whether they were committed or had local edits.
cp Cargo.toml Cargo.toml.shadow-bak
cp Cargo.lock Cargo.lock.shadow-bak

restore() {
    mv -f Cargo.toml.shadow-bak Cargo.toml
    mv -f Cargo.lock.shadow-bak Cargo.lock
}
trap restore EXIT

./shadow/inject-patch.sh

"$@"
