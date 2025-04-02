#!/usr/bin/env bash
set -eo pipefail

BOOK_ROOT="$(dirname "$(dirname "$0")")"
REAM=${1:-"$(dirname "$BOOK_ROOT")/target/debug/ream"}

cmd=(
  "$(dirname "$0")/help.rs"
  --root-dir "$BOOK_ROOT/"
  --root-indentation 2
  --root-summary
  --out-dir "$BOOK_ROOT/cli/"
  "$REAM"
)
echo "Running: $" "${cmd[*]}"
"${cmd[@]}"
