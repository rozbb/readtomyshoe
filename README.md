# ReadToMyShoe

A website that reads articles to you, even when you're offline.

This is a full stack Rust web app using [axum](https://github.com/tokio-rs/axum) and [yew](https://yew.rs/).

## Installation

### Google Cloud

We use Google Cloud's text to speech engine. Here's how to get an API key:

* Make a [Google Cloud](https://cloud.google.com) account,
* Go to the [credentials page](https://console.cloud.google.com/apis/credentials)
* Click "Create Credentials" at the top of the page and select "API Key"
* You can copy the API key right now or click "Edit API key" and restrict its capabilities. To restrict:
    * Click "Edit API key"
    * Under "API restrictions" click "Restrict key"
    * Select "Cloud Text-to-Speech API"
* Now copy your API key to the clipboard
* Whenever you run the server, you need to set the environment variable `GCP_API_KEY` to your API key

### Other setup

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
