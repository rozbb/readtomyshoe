use super::media_session::MediaSessionState;
use crate::WeakComponentLink;

use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{Blob, HtmlAudioElement, MediaSessionActionDetails, Url};
use yew::prelude::*;

/// The ID of the unique audio element the page
pub const AUDIO_ELEM_ID: &str = "mainAudio";

// If an audio jump offset isn't set, jump by 10 seconds
const DEFAULT_JUMP_SIZE: f64 = 10.0;

/// Holds operations we can do on the unique <audio> element on this page
pub struct GlobalAudio;

impl GlobalAudio {
    /// Helper function to retrieve the only audio element from the page
    fn get_elem() -> HtmlAudioElement {
        gloo_utils::document()
            .get_element_by_id(AUDIO_ELEM_ID)
            .unwrap()
            .dyn_into()
            .unwrap()
    }

    /// Seeks to the specified time
    pub fn seek(time: f64) {
        let audio_elem = GlobalAudio::get_elem();
        audio_elem.set_current_time(time);
    }

    /// Gets the current elapsed time, in seconds
    pub fn get_elapsed() -> f64 {
        let audio_elem = GlobalAudio::get_elem();
        audio_elem.current_time()
    }

    /// Gets the current playback speed
    pub fn get_playback_speed() -> f64 {
        let audio_elem = GlobalAudio::get_elem();
        audio_elem.playback_rate()
    }

    /// Fast-seeks to the specified time
    pub fn fast_seek(time: f64) {
        let audio_elem = GlobalAudio::get_elem();
        audio_elem.fast_seek(time).expect("fast seek failed");
    }

    /// Jumps forward or backwards by the specified offset
    pub fn jump_offset(offset: f64) {
        let audio_elem = GlobalAudio::get_elem();

        // New time must be in the range [0, duration]
        let new_time = f64::min(audio_elem.duration(), audio_elem.current_time() + offset);
        let new_time = f64::max(0.0, new_time);

        audio_elem.set_current_time(new_time);
    }

    /// Jumps forward by JUMP_SIZE seconds
    pub fn jump_forward(details: MediaSessionActionDetails) {
        let seek_offset =
            js_sys::Reflect::get(&details, &JsValue::from_str("seekOffset")).map(|t| t.as_f64());

        // If the offset isn't given, use the default jump size
        let seek_offset = match seek_offset {
            Ok(Some(off)) => off,
            _ => DEFAULT_JUMP_SIZE,
        };

        tracing::trace!("Jumping forward {} seconds", seek_offset);
        GlobalAudio::jump_offset(seek_offset);
    }

    /// Jumps backward by JUMP_SIZE seconds
    pub fn jump_backward(details: MediaSessionActionDetails) {
        let seek_offset =
            js_sys::Reflect::get(&details, &JsValue::from_str("seekOffset")).map(|t| t.as_f64());

        // If the offset isn't given, use the default jump size
        let seek_offset = match seek_offset {
            Ok(Some(off)) => off,
            _ => DEFAULT_JUMP_SIZE,
        };

        tracing::trace!("Jumping backward {} seconds", seek_offset);
        GlobalAudio::jump_offset(-seek_offset);
    }

    // A helper function that plays empty audio. This is necessary because of a quirk in Safari that
    // doesn't let async functions be the first thing to call play()
    pub async fn fake_play() {
        let audio_elem = GlobalAudio::get_elem();

        // Set the source to empty, so nothing gets played
        GlobalAudio::set_source(&Blob::new().unwrap());

        // Now play the empty blob. This will error but we don't care.
        let promise = audio_elem.play().unwrap();
        let _ = JsFuture::from(promise).await;
    }

    /// Runs play() on the <audio> element in this page
    pub async fn play() {
        tracing::trace!("Playing audio");
        let audio_elem = GlobalAudio::get_elem();

        let promise = audio_elem.play().unwrap();
        let res = JsFuture::from(promise).await;
        tracing::trace!("Played audio");
        if let Err(e) = res {
            tracing::error!("Error playing track: {:?}", e);
        }
    }

    /// Returns whether something is currently playing
    pub fn is_playing() -> bool {
        let audio_elem = GlobalAudio::get_elem();
        // If there is something loaded and the state is not paused, then we're playing something
        audio_elem.duration() > 0.0 && !audio_elem.paused()
    }

    /// Runs pause() on the <audio> element in this page
    pub fn pause() {
        let audio_elem = GlobalAudio::get_elem();
        audio_elem.pause().unwrap();
    }

    /// Stops playback. This pauses the audio and unloads the source
    pub fn stop() {
        GlobalAudio::pause();
        GlobalAudio::seek(0.0);
        GlobalAudio::set_source(&Blob::new().unwrap());
    }

    /// Sets the <audio>'s src to the given article's MP3 blob
    pub fn set_source(blob: &Blob) {
        // Pause the current
        let audio_elem = GlobalAudio::get_elem();
        audio_elem.pause().unwrap();

        // Initialize the MediaSession with metadata and callbacks

        // Construct a URL that refers to the blob. This will be the audio player's src attribute
        let blob_url = Url::create_object_url_with_blob(&blob).unwrap();
        // Set the src
        audio_elem.set_src(&blob_url);
    }

    /// Sets the playback speed of the <audio> tag and updates the speed selection combobox
    pub fn set_playback_speed(speed: f64) {
        let audio_elem = GlobalAudio::get_elem();

        // Set the playback rate in the <audio> element
        audio_elem.set_playback_rate(speed);
        audio_elem.set_default_playback_rate(speed);
    }

    /// Sets the callback for the `canplay` event, which triggers when the audio is determined to
    /// be playable, but not enough has been loaded yet.
    pub fn set_canplay_cb(cb: &Closure<dyn Fn(Event)>) {
        let audio_elem = GlobalAudio::get_elem();

        let func = cb.as_ref().unchecked_ref();
        if let Err(e) = audio_elem.add_event_listener_with_callback("canplay", &func) {
            tracing::error!("Could not set canplay callback: {:?}", e);
        }
    }

    /// Removes the callback for the `canplay` event, which triggers when the audio is determined to
    /// be playable, but not enough has been loaded yet.
    pub fn remove_canplay_cb(cb: &Closure<dyn Fn(Event)>) {
        let audio_elem = GlobalAudio::get_elem();

        let func = cb.as_ref().unchecked_ref();
        if let Err(e) = audio_elem.remove_event_listener_with_callback("canplay", &func) {
            tracing::error!("Could not remove canplay callback: {:?}", e);
        }
    }

    /// Sets the callback for the `ratechange` event, which triggers when the audio's playback
    /// speed has been chagned
    pub fn set_ratechange_cb(cb: &Closure<dyn Fn(Event)>) {
        let audio_elem = GlobalAudio::get_elem();

        let func = cb.as_ref().unchecked_ref();
        if let Err(e) = audio_elem.add_event_listener_with_callback("ratechange", &func) {
            tracing::error!("Could not set ratechange callback: {:?}", e);
        }
    }
}

/*
async fn sleep(millis: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, millis)
            .unwrap();
    });
    JsFuture::from(promise).await.unwrap();
}
*/

#[derive(PartialEq, Properties)]
pub struct Props {
    /// A link to myself. We have to set this on creation
    pub audio_link: WeakComponentLink<Audio>,
}

pub enum AudioMsg {
    /// Load the given blob and set start time to `elapsed` seconds
    Load {
        src: Blob,
        title: String,
        elapsed: f64,
    },

    /// Load the given blob and play from `elapsed` seconds
    Play,

    /// Jump forward `JUMP_SIZE` seconds
    JumpForward,

    /// Jump backward `JUMP_SIZE` seconds
    JumpBackward,

    /// Sets the audio playback speed to the given percentage
    SetPlaybackSpeed(f64),

    /// **INTERNAL:** Sets the elapsed time in the currently loaded audio. Do not use
    _SetElapsed(f64),

    /// Stop playback
    Stop,
}

/// Callbacks that are triggered by events from the <audio> element
#[derive(Default)]
struct AudioElemCallbacks {
    /// The closure that runs whenever the <audio> element has loaded its source
    _canplay_cb: Option<Closure<dyn Fn(Event)>>,
    /// The closure that runs whenever the <audio> element's playback speed has changed
    _ratechange_cb: Option<Closure<dyn Fn(Event)>>,
}

/// A component that's just an HTML <audio> element with some extra functionality
#[derive(Default)]
pub struct Audio {
    audio_elem_cbs: AudioElemCallbacks,
}

impl Component for Audio {
    type Message = AudioMsg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        // Set the player link to this Player
        ctx.props()
            .audio_link
            .borrow_mut()
            .replace(ctx.link().clone());

        Self::default()
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            AudioMsg::Load {
                src,
                title,
                elapsed,
            } => {
                // Set the audio source and title
                GlobalAudio::set_source(&src);
                MediaSessionState::set_title(&title);

                // Register the closure that runs whenever the audio's source is loaded
                let link = ctx.link().clone();
                let cb = Closure::new(move |_: Event| {
                    // When the audio is loaded, set the elapsed time
                    link.send_message(AudioMsg::_SetElapsed(elapsed))
                });
                GlobalAudio::set_canplay_cb(&cb);
                // Save the callback so it doesn't go out of scope
                self.audio_elem_cbs._canplay_cb = Some(cb);
            }

            AudioMsg::Play => {
                spawn_local(async move {
                    GlobalAudio::play().await;
                });
            }

            AudioMsg::JumpForward => {
                GlobalAudio::jump_offset(DEFAULT_JUMP_SIZE);
            }

            AudioMsg::JumpBackward => {
                GlobalAudio::jump_offset(-DEFAULT_JUMP_SIZE);
            }

            AudioMsg::_SetElapsed(elapsed) => {
                // This message only comes from the canplay callback. We're about to do a seek,
                // which triggers canplay again. To avoid infinite recursion, remove the callback
                GlobalAudio::remove_canplay_cb(
                    self.audio_elem_cbs
                        ._canplay_cb
                        .as_ref()
                        .expect("got _SetElapsed from outside a canplay callback"),
                );

                // Seek to the desired position and update the MediaSession scrubber
                GlobalAudio::seek(elapsed);
            }

            AudioMsg::SetPlaybackSpeed(speed) => {
                GlobalAudio::set_playback_speed(speed);
            }

            AudioMsg::Stop => {
                GlobalAudio::stop();
                // Nothing is playing anymore. Clear the metadata
                MediaSessionState::clear();
            }
        }

        false
    }

    fn view(&self, _ctx: &Context<Self>) -> Html {
        html! {
            <audio controls=true style={ "display: block;" } id={AUDIO_ELEM_ID}>
                { "Your browser does not support the <code>audio</code> element" }
            </audio>
        }
    }
}
