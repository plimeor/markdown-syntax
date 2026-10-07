#!/usr/bin/env bash
# Checks the build invariants CI does not run, before a release: the crate
# builds for wasm32-unknown-unknown and with Rust 1.82 (the MSRV), with default
# features and with `html`.
#
# Needs `rustup target add wasm32-unknown-unknown` and
# `rustup toolchain install 1.82` once.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --target wasm32-unknown-unknown
cargo build --target wasm32-unknown-unknown --features html
cargo +1.82 build
cargo +1.82 build --features html
echo "pre-release check passed: wasm32 and Rust 1.82 builds, default and html"
