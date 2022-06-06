# ReadToMyShoe

A website that reads articles to you, even when you're offline.

This is a full stack Rust web app using [axum](https://github.com/tokio-rs/axum) and [yew](https://yew.rs/).

## Installation

This requires a custom version of everything related wasm-bindgen because the `MediaSession` API has not yet been merged into `web_sys`. Do the following:

* `cargo install --locked trunk wasm-bindgen-cli`
* Set the following in `~/.cargo/config`. This is because `web_sys` gates unstable features via cfg. I know this is very weird.
```
[build]
rustflags = ["--cfg=web_sys_unstable_apis"]
rustdocflags = ["--cfg=web_sys_unstable_apis"]
```

## Usage

Run the dev version (auto-reloads server & client on file change) with `./scripts/dev.sh`.

Run the pre-compiled version with `./scripts/prod.sh`.

The app will start at https://localhost:8080 by default.

## Thanks

A lot of the ideas and code in this crate started with Robert Krahn's [fantastic template](https://robert.kra.hn/posts/2022-04-03_rust-web-wasm/#making-the-file-server-support-a-spa-app). Thanks
