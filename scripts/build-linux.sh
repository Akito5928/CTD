#!/bin/sh
# Build, test and package the Linux x86_64 executable.
# Usage: scripts/build-linux.sh [test|build|bench]   (default: build)
set -eu
cd "$(dirname "$0")/.."
VERSION=1.1.0
MODE="${1:-build}"

case "$MODE" in
  test)
    cargo test --release --locked
    ;;
  bench)
    cargo build --release --locked
    cargo run --release --locked --example bench -- --sizes 1000,10000,100000 | tee bench/results-linux.md
    ;;
  build)
    cargo test --release --locked
    cargo build --release --locked
    mkdir -p dist
    cp target/release/ctd "dist/ctd-v${VERSION}-linux-x86_64"
    echo "Built: dist/ctd-v${VERSION}-linux-x86_64"
    ;;
  *)
    echo "usage: $0 [test|build|bench]" >&2
    exit 2
    ;;
esac
