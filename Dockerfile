# This file modified from
# https://github.com/fly-apps/hello-rust/blob/70afce57cfd27e636c849860f0a4f7be76127049/Dockerfile

FROM rust:latest as builder

# Make a fake Rust app to keep a cached layer of compiled crates
RUN USER=root cargo new app
WORKDIR /usr/src/app
COPY Cargo.toml Cargo.lock ./
RUN mkdir frontend server common
COPY frontend/Cargo.toml frontend/
COPY server/Cargo.toml server/
COPY common/Cargo.toml common/

# Needs at least a main.rs file with a main function
RUN mkdir frontend/src && echo "fn main(){}" > frontend/src/main.rs
RUN mkdir server/src && echo "fn main(){}" > server/src/main.rs
RUN mkdir common/src && echo "fn main(){}" > common/src/main.rs

# Will build all dependent crates in release mode
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/src/app/target \
    cargo build --release
# Will install the tools needed to build the webapp
RUN rustup target add wasm32-unknown-unknown
RUN cargo install --locked trunk

# Copy the rest.
COPY . .
# Build (install) the actual binaries
RUN RUSTFLAGS=--cfg=web_sys_unstable_apis ./scripts/prodbuild.sh

# Runtime image
FROM debian:bullseye-slim

# Need certs for talking to Google Cloud
RUN \
  apt-get update && \
  apt-get install -y ca-certificates && \
  apt-get clean

# Run as "app" user
RUN useradd -ms /bin/bash app

USER app
WORKDIR /app

# The binary and the blobs dir need to be in server/
RUN mkdir server
RUN mkdir server/audio_blobs

# Get compiled binaries and assets from the builder's cargo install directory
COPY --from=builder /usr/src/app/dist /app/dist
COPY --from=builder /usr/local/cargo/bin/readtomyshoe-server /app/server/readtomyshoe-server

# Copy the API key
# NOTE: This is a secret value!
COPY ./server/gcp_api.key ./server/

# Go to where the binary is
WORKDIR /app/server

CMD ./readtomyshoe-server --port 8080 --addr 0.0.0.0
