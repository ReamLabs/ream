#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PATCH_FILE="$ROOT/shadow/cargo-patch.toml"

trap 'rm -f Cargo.toml.shadow-tmp' EXIT

if grep -q '^\[patch\.crates-io\]' Cargo.toml; then

    awk -v patch_file="$PATCH_FILE" '
        { print }
        /^\[patch\.crates-io\]$/ && !injected {
            while ((getline line < patch_file) > 0) print line
            close(patch_file)
            injected = 1
        }
    ' Cargo.toml > Cargo.toml.shadow-tmp
    mv Cargo.toml.shadow-tmp Cargo.toml
else
    printf '\n[patch.crates-io]\n%s\n' "$(cat "$PATCH_FILE")" >> Cargo.toml
fi
