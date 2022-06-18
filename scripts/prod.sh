#!/usr/bin/env bash
set -euo pipefail
IFS=$'\n\t'

pushd frontend
RUSTFLAGS=--cfg=web_sys_unstable_apis trunk build --public-url /assets/
popd

pushd server
cargo run --release -- --port 8080 --address 0.0.0.0
popd
