#!/usr/bin/env bash
set -euo pipefail

awk '
BEGIN {
  serial = 0
  ignored = 0
}
/^[[:space:]]*#\[serial\][[:space:]]*$/ {
  serial = 1
  ignored = 0
  next
}
serial && /^[[:space:]]*#\[ignore([[:space:]]*=.*)?\][[:space:]]*$/ {
  ignored = 1
  next
}
serial && /^[[:space:]]*fn[[:space:]]+[A-Za-z_][A-Za-z0-9_]*[[:space:]]*\(/ {
  if (!ignored) {
    name = $0
    sub(/^[[:space:]]*fn[[:space:]]+/, "", name)
    sub(/[[:space:]]*\(.*/, "", name)
    print name
  }
  serial = 0
  ignored = 0
  next
}
serial && /^[[:space:]]*#\[/ {
  next
}
serial && !/^[[:space:]]*$/ {
  serial = 0
  ignored = 0
}
' bin/ream/src/main.rs
