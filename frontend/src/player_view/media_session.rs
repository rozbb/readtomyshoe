use super::audio_component::GlobalAudio;

use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use wasm_bindgen_futures::spawn_local;
use web_sys::{
    MediaImage, MediaMetadata, MediaSession, MediaSessionAction, MediaSessionActionDetails,
};

/// Use the RTMS logo as the album image
const ALBUM_IMAGE_URL: &str = "/assets/rtms-color-512x512.png";

/// Helper function to retrieve the MediaSession API
fn get_media_session() -> MediaSession {
    gloo_utils::window().navigator().media_session()
}

/// These are the callbacks the browser calls when the user performs a MediaSession operation like
/// seeking forward or skipping a track
pub struct MediaSessionCallbacks {
    _play_action: Closure<dyn Fn()>,
    _pause_action: Closure<dyn Fn()>,
    _seek_to_action: Closure<dyn Fn(MediaSessionActionDetails)>,
    _jump_forward_action: Closure<dyn Fn(MediaSessionActionDetails)>,
    _jump_backward_action: Closure<dyn Fn(MediaSessionActionDetails)>,
    _next_track_action: Closure<dyn Fn()>,
    _prev_track_action: Closure<dyn Fn()>,
}

impl Default for MediaSessionCallbacks {
    /// Sets all the callbacks necessary for the MediaSession to be usable. On an iPhone, this will
    /// cause the following controls to be displayed on the lockscreen: play/pause, jump back, jump
    /// forward, scrobble.
    fn default() -> Self {
        let _play_action = Closure::new(|| {
            spawn_local(async move {
                GlobalAudio::play().await;
            })
        });
        let _pause_action = Closure::new(|| GlobalAudio::pause());
        let _seek_to_action = Closure::new(MediaSessionState::seek_to);
        let _jump_forward_action = Closure::new(GlobalAudio::jump_forward);
        let _jump_backward_action = Closure::new(GlobalAudio::jump_backward);

        // The next track button currently does nothing
        let _next_track_action = Closure::new(|| ());
        // The prev track button just resets the playback to the beginning
        let _prev_track_action = Closure::new(|| GlobalAudio::seek(0.0));

        let media_session = get_media_session();

        // Set all the callbacks
        media_session.set_action_handler(
            MediaSessionAction::Play,
            Some(_play_action.as_ref().unchecked_ref()),
        );
        media_session.set_action_handler(
            MediaSessionAction::Pause,
            Some(_pause_action.as_ref().unchecked_ref()),
        );
        media_session.set_action_handler(
            MediaSessionAction::Seekto,
            Some(_seek_to_action.as_ref().unchecked_ref()),
        );
        media_session.set_action_handler(
            MediaSessionAction::Seekforward,
            Some(_jump_forward_action.as_ref().unchecked_ref()),
        );
        media_session.set_action_handler(
            MediaSessionAction::Seekbackward,
            Some(_jump_backward_action.as_ref().unchecked_ref()),
        );
        media_session.set_action_handler(
            MediaSessionAction::Nexttrack,
            Some(_next_track_action.as_ref().unchecked_ref()),
        );
        media_session.set_action_handler(
            MediaSessionAction::Previoustrack,
            Some(_prev_track_action.as_ref().unchecked_ref()),
        );

        MediaSessionCallbacks {
            _play_action,
            _pause_action,
            _seek_to_action,
            _jump_forward_action,
            _jump_backward_action,
            _next_track_action,
            _prev_track_action,
        }
    }
}

pub struct MediaSessionState;

impl MediaSessionState {
    /// Clears the metadata of the session. This means nothing is playing
    pub fn clear() {
        let media_session = get_media_session();
        media_session.set_metadata(None);
    }

    /// Sets the MediaSession title of the currently playing track
    pub fn set_title(title: &str) {
        let media_session = get_media_session();

        // Set the title
        let metadata = MediaMetadata::new().unwrap();
        metadata.set_title(&title);

        // Set the artwork. It's an array consisting of just 1 image
        let artwork = js_sys::Array::new_with_length(1);
        let mut image = MediaImage::new(ALBUM_IMAGE_URL);
        image.sizes("any");
        image.type_("image/png");
        artwork.set(0, image.into());
        metadata.set_artwork(&artwork);

        media_session.set_metadata(Some(&metadata));
    }

    // TODO: use wasm_bindgen generated getters to get fields from these dicts. This is blocked on
    // https://github.com/rustwasm/wasm-bindgen/issues/2921
    /// Callback for the "seekto" MediaSession action
    fn seek_to(details: MediaSessionActionDetails) {
        let fast_seek =
            js_sys::Reflect::get(&details, &JsValue::from_str("fastSeek")).map(|t| t.as_bool());
        let seek_time =
            js_sys::Reflect::get(&details, &JsValue::from_str("seekTime")).map(|t| t.as_f64());
        let seek_offset =
            js_sys::Reflect::get(&details, &JsValue::from_str("seekOffset")).map(|t| t.as_f64());

        tracing::trace!(
            "Seeking to offset {:?} or time {:?}",
            seek_offset,
            seek_time
        );

        // Seek to the specified time, if defined
        if let Ok(Some(time)) = seek_time {
            // If "fast seek" is set, us that method
            match fast_seek {
                Ok(Some(true)) => GlobalAudio::fast_seek(time),
                _ => GlobalAudio::seek(time),
            }
        } else if let Ok(Some(off)) = seek_offset {
            GlobalAudio::jump_offset(off);
        }
    }
}
