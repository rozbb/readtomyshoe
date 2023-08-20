<p align="center">
    <img width="300" height="300" src="logos/rtms-color-512x512.png" alt="ReadToMyShoe logo: A sneaker wearing a headset with a microphone">
</p>

# ReadToMyShoe

[**Video Demo**](https://www.dropbox.com/s/7i65qyv2i9uosp5/readtomyshoe_demo.mp4?dl=0)

ReadtoMyShoe (RTMS) is a web app that lets you upload articles (via URL or via directly pasting) and listen to them later. Some features:

* **High-quality text-to-speech:** RTMS uses the Google Cloud Text to Speech [WaveNet voices](https://cloud.google.com/text-to-speech/docs/wavenet). It's not quite human yet, but it's pretty nice.
* **Listen as a podcast:** To listen to your articles from your favorite podcast app, just add `INSTANCE/api/feed.xml`.
* **Web version features:**
    * **Offline-first:** All the articles in your queue are available offline. The web version of RTMS is usable even in airplane mode.
    * **Saves your progress:** Don't lose your place in your reading material. RTMS will save where you are. So next time you play an article, it'll resume right where you left off.
    * **Lockscreen controls:** Play, pause, jump 10 seconds. It's all available from the lock screen or notification bar of your mobile device.
    * **Runs anywhere:** Since RTMS is a web app, it runs everywhere a (modern) web browser runs.
    * **Add to Homescreen:** RTMS can be added to your homescreen and behave just like a native app.

RTMS is written in Rust, using [yew](https://yew.rs/) for the frontend (compiles to WASM) and [axum](https://github.com/tokio-rs/axum) for the backend.

## Usage

To access the **web** interface, simply navigate to your instance URL in your web browser. From there, you can add articles or listen to them in-browser.

You can also use the **podcast** interface to listen to articles. Simply add `INSTANCE/api/feed.xml` to your favorite podcast app (where `INSTANCE` is your instance's URL).

## Limitations

ReadToMyShoe uses some browser features that are new and/or buggy. Some limitations of the web app are:

* Does not work in private mode. In Firefox and Safari, RTMS will not let you Add to Queue. This is because you cannot touch local storage from a private browsing window.
* Lockscreen controls are broken in Firefox for Android. You can still play audio in Firefox for Android, but play/pause, seek, and jump buttons are all missing.
* Add to Homescreen is not very functional in iOS. This is a documented Safari bug. [Issue](https://github.com/rozbb/readtomyshoe/issues/4). Just use the website from within Safari.

## Accessibility

It is important that ReadToMyShoe be accessible to the visually impaired and others who rely on text-to-speech for reading. If you have an accessibility issue while using ReadToMyShoe, please open up a Github Issue at [this link](https://github.com/rozbb/readtomyshoe/issues/new). If you don't have a Github account, please email me at rtms-a11y@mrosenberg.pub

## Running your own instance

To set up your own instance of ReadToMyShoe, check out the [Getting Started](https://github.com/rozbb/readtomyshoe/wiki/Getting-Started) page in the wiki.

## Licenses

All code is licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE))
 * MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Images are licensed by Michael Rosenberg under the [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/).

## Thanks

A lot of the ideas and code in this crate started with Robert Krahn's [fantastic template](https://robert.kra.hn/posts/2022-04-03_rust-web-wasm/#making-the-file-server-support-a-spa-app). Thanks

Also, big thanks to my friend Sharon Ye for her immense help in the design of the logo.
