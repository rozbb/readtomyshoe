<p align="center">
    <img width="300" height="300" src="logos/rtms-color-512x512.png" alt="ReadToMyShoe logo: A sneaker wearing a headset with a microphone">
</p>

# ReadToMyShoe

[**Video Demo**](https://www.dropbox.com/s/7i65qyv2i9uosp5/readtomyshoe_demo.mp4?dl=0)

A website that reads articles to you, even when you're offline. Still in early development.

This is a full-stack Rust webapp, using [axum](https://github.com/tokio-rs/axum) for the backend and [yew](https://yew.rs/) for the frontend.

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

Once you've got your API key, you need to give it to the ReadToMyShoe server:

* Create a new file in the `server/` directory, called `gcp_api.key`
* Paste the API key into that file

That's it!

### OpenSSL

You need to have OpenSSL installed on your machine:

* For Ubuntu and Debian, do `sudo apt-get install libssl-dev`
* For Mac, do `brew install openssl`

### Python dependencies

We use `trafilatura` for article content extraction. This is installed directly in the local directory.

* Install `pip3`. On a Mac: `brew install python3`. On Ubuntu: `sudo apt-get install python3-pip`.
* In `readtomyshoe/`, run `pip3 install trafilatura -t python_deps`
    * In the future, if you want to update `trafilatura`, run `pip3 install --upgrade trafilatura -t python_deps`

ReadToMyShoe will now be able to use the `python_deps/bin/trafilatura` binary.

**For devs:** If you wanna run `trafilatura` yourself, first `cd python_deps`, then run `PYTHONPATH=. ./bin/trafilatura`

### Other necessary setup

We need a few utilities for building the website.

* First, make sure you have rustup installed. Follow the directions [here](https://rustup.rs/). Make sure to add `$HOME/.cargo/bin` to your `PATH`.
* Run `rustup target add wasm32-unknown-unknown`. This installs the WASM target so Rust knows how to output code for the browser.
* `cargo install --locked trunk`. The `trunk` utility packages the frontend assets of the website.
* If you're on an M1 Mac, run  `cargo install --locked wasm-bindgen-cli`. This is because `trunk` doesn't know how to download the bindgen CLI itself ([tracking issue](https://github.com/thedodd/trunk/pull/375)).

### Dev Setup

If you want to work on this repo in an IDE, it will be easiest if you put the following lines in your `~/.cargo/config` file. This is because `web_sys` gates unstable features via cfg. I know this is very weird.
```
[build]
rustflags = ["--cfg=web_sys_unstable_apis"]
rustdocflags = ["--cfg=web_sys_unstable_apis"]
```

You'll also need to run `cargo install cargo-watch` in order to use `script/dev.sh`. This is a filesystem watcher that tells the server to autoreload whenever a file is changed.

## Usage

Run the dev version (auto-reloads server & client on file change) with `./scripts/dev.sh`.

Run the pre-compiled version with `./scripts/prod.sh`.

The app will start at `https://localhost:8080` by default. **The default behavior is to make the service visible to your whole local network.** To make it only accessible from your own machine, delete `--address 0.0.0.0` from `dev.sh` and/or `prod.sh`.

**NOTE:** If you run ReadToMyShoe on your local network or generally without HTTPS, iOS will NOT cache anything. This is because Service Workers are only allowed in "secure contexts" (meaning via HTTPS or on localhost).

## Deployment

**Docker:** The `Dockerfile` in this directory should Just Work. Run `docker run readtomyshoe -p 8080:8080` to start ReadToMyShoe on port 8080. Currently, this does not support storing audio files on an external volume. This is in the TODOs.

**Fly.io:** We also support one-click deployment on [Fly.io](https://fly.io). To deploy, simply pick a new app name in `fly.toml`, make an app with that name in your Fly.io account, and run `flyctl deploy`. This uses the Dockerfile for deployment, so any changes there will change your Fly.io deployment.

## TODO

An incomplete to-do list, roughly in order of most important to least important:

- [ ] Make playqueue reset time if the article is done(?)
- [ ] Add an archive feature for articles whose audio has been pruned
- [ ] Make it so that deleting an article from the queue deletes all its copies from the queue
- [ ] Write more accessible progress notifs for adding to queue, and error notifs for fetch and what not
- [ ] Make the Dockerfile compatible with external volumes
- [ ] Fix caching in dev mode (the trouble is that assets are in `/assets/` in prod, but `/` in dev)
- [ ] Implement login functionality and make per-user libraries
- [ ] Write rate-limiting code for TTS service
- [ ] The PWA reloads nonstop when the server is down. Service worker should return cache on fetch error
- [ ] Use `<audio>`'s onratechange event to set the playback combobox to the correct value
- [ ] Make "fake play" (ie playing nothing before playing the real file to avoid Safari's weird anti-autoplay rules) not throw an error. It currently errors in every browser because it plays the empty blob, which is invalid.
- [ ] Fix the ugly error on Safari private mode. I think this should just detect the error and say Safari private mode isn't supported
- [ ] Support share target (no iOS support) https://web.dev/web-share-target/
- [ ] Add article by PDF
- [ ] Add article by batch URL
- [ ] Add login support for WSJ, NYT, Bloomberg, SEC EDGAR (needs [user agent](https://www.sec.gov/os/webmaster-faq#code-support))

## Licenses

All code is licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE))
 * MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Images are licensed by Michael Rosenberg under the [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/).

## Thanks

A lot of the ideas and code in this crate started with Robert Krahn's [fantastic template](https://robert.kra.hn/posts/2022-04-03_rust-web-wasm/#making-the-file-server-support-a-spa-app). Thanks

Also, big thanks to my friend Sharon Ye for her immense help in the design of the logo.
