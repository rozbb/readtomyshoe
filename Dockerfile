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

# Copy the rest
COPY . .
# Build (install) the actual binaries
RUN ./scripts/prodbuild.sh

# Runtime image
FROM debian:bullseye-slim

# Run as "app" user
RUN useradd -ms /bin/bash app

USER app
WORKDIR /app

# Get compiled binaries from builder's cargo install directory
COPY --from=builder /usr/local/cargo/bin/readtomyshoe-server /app/readtomyshoe-server

# No CMD or ENTRYPOINT, see fly.toml with `cmd` override.
