#!/usr/bin/env bash
set -euo pipefail
IFS=$'\n\t'

pushd frontend
RUSTFLAGS=--cfg=web_sys_unstable_apis trunk build --release --public-url /assets/
popd

pushd server
cargo run --release -- --port 9382
popd

# Full backend options. You can get these by running `cargo run --release -- --help` in server/
#
# USAGE:
#     readtomyshoe-server [OPTIONS]
#
# OPTIONS:
#     -a, --addr <ADDR>
#             The listen addr [default: ::1]
#
#         --audio-blob-dir <AUDIO_BLOB_DIR>
#             The directory where the audio blobs are stored [default: audio_blobs]
#
#     -h, --help
#             Print help information
#
#     -i, --index <INDEX_FILE>
#             The default file to serve, aka index.html [default: index.html]
#
#     -l, --log <LOG_LEVEL>
#             The log level [default: debug]
#
#         --max-chars-per-min <MAX_CHARS_PER_MIN>
#             The limit on the number of article characters (bytes, really) the server will process
#             per minute. The default is 5M because that's Google Cloud's limit. Use large values with
#             caution: a malicious user can rack up your Google Cloud costs [default: 5000000]
#
#     -p, --port <PORT>
#             The listen port [default: 9382]
#
#         --static-dir <STATIC_DIR>
#             The directory where static files are to be found [default: ../dist]
