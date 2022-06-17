#!/usr/bin/env bash
set -euo pipefail
IFS=$'\n\t'

pushd frontend
trunk build --public-url /assets/
popd

pushd server
cargo install --path .
popd
