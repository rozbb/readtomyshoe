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

Once you've got your API key, you need to give it to the readtomyshoe server:

* Create a new file in the `server/` directory, called `gcp_api.key`
* Paste the API key into that file

That's it!

### OpenSSL

You need to have OpenSSL installed on your machine:

* For Ubuntu and Debian, do `sudo apt-get install libssl-dev`
* For Mac, do `brew install openssl`

### Python dependencies

We use `trafilatura` for article content extraction. This is installed directly in the local directory. Make sure you have `pip3` installed, then run:

```
pip3 install trafilatura -t python_deps
```

ReadToMyShoe will now be able to use the `python_deps/bin/trafilatura` binary.

**For devs:** If you wanna run `trafilatura` yourself, first `cd python_deps`, then run `PYTHONPATH=. ./bin/trafilatura`
 
### Other setup

We need a few utilities for building the website. Run the following:

* `rustup target add wasm32-unknown-unknown`. This installs the WASM target so Rust knows how to output code for the browser.
* `cargo install --locked trunk`. The `trunk` utility packages the frontend assets of the website.
* If you're on an M1 Mac, run  `cargo install --locked wasm-bindgen-cli`. This is because `trunk` doesn't know how to download the bindgen CLI itself ([tracking issue](https://github.com/thedodd/trunk/pull/375)).

### Dev Setup

If you want to work on this repo in an IDE, it will be easiest if you put the following lines in your `~/.cargo/config` file. This is because `web_sys` gates unstable features via cfg. I know this is very weird.
```
[build]
rustflags = ["--cfg=web_sys_unstable_apis"]
rustdocflags=["--cfg=web_sys_unstable_apis"]
```

You'll also need to run `cargo install cargo-watch` in order to use `script/dev.sh`. This is a filesystem watcher that tells the server to autoreload whenever a file is changed.

## Usage

Run the dev version (auto-reloads server & client on file change) with `./scripts/dev.sh`.

Run the pre-compiled version with `./scripts/prod.sh`.

The app will start at `https://localhost:8080` by default. **The default behavior is to make the service visible to your whole local network.** To make it only accessible from your own machine, delete `--address 0.0.0.0` from `dev.sh` and/or `prod.sh`.

## TODO

An incomplete to-do list, roughly in order of most important to least important:

- [ ] Save current article, playback position, and speed
- [ ] Write an accessible progress notifs for adding to queue, and error notifs for fetch and what not
- [ ] Sort library by date added
- [ ] Implement "next track" functionality with queue
- [ ] Handle errors in article fetch better: delete temp files, implement retries to Google TTS
- [ ] Save more metadata than article title: id, url, title, byline, text. Maybe store in the ID3v2 tags?
- [ ] Show more detailed progress in the add article view
- [ ] Make whole site WCAG-compliant
- [ ] Make queue order mutable
- [ ] Fix caching in dev mode (the trouble is that assets are in `/assets/` in prod, but `/` in dev)
- [ ] Implement login functionality and make per-user libraries
- [ ] Write rate-limiting code for TTS service

## Thanks

A lot of the ideas and code in this crate started with Robert Krahn's [fantastic template](https://robert.kra.hn/posts/2022-04-03_rust-web-wasm/#making-the-file-server-support-a-spa-app). Thanks
