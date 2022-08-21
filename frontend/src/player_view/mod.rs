mod audio_component;
mod media_session;

use crate::{
    caching,
    queue_view::{ArticleId, Queue, QueueEntry, QueueMsg},
    utils, WeakComponentLink,
};
use audio_component::{Audio, AudioMsg, GlobalAudio};
use media_session::MediaSessionCallbacks;

use serde::{Deserialize, Serialize};
use wasm_bindgen::{closure::Closure, JsCast};
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlSelectElement;
use yew::{html::Scope, prelude::*};

const SPEED_SELECTOR_ID: &str = "speed-selector";

// The number of milliseconds between times saving Player state
const PLAYER_STATE_SAVE_FREQ: i32 = 10000;

/// All the playback speeds we support
const PLAYBACK_SPEEDS: &[f64] = &[0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0];

/// Loads the given article and its playback state, and sets the <audio>'s src to the MP3 blob.
/// Returns the desired elapsed time for the article.
async fn prepare_for_play(id: &ArticleId, audio_link: &Scope<Audio>) -> f64 {
    // Load the article state and set the elapsed time.
    let elapsed = match caching::load_article_state(&id).await {
        Ok(state) => state.elapsed,
        Err(e) => {
            tracing::debug!("Article state did not load {}: {:?}", id.0, e);
            0.0
        }
    };

    // Load the article and set the <audio> src to it
    match caching::load_article(&id).await {
        Ok(article) => {
            let mp3_blob = utils::bytes_to_mp3_blob(&article.audio_blob);
            audio_link.send_message(AudioMsg::Load {
                src: mp3_blob,
                title: article.title,
                elapsed,
            });
        }
        Err(e) => {
            tracing::error!("Couldn't load article {}: {:?}", id.0, e);
        }
    }

    elapsed
}

/// Returns the combobox used to select playback speed
fn get_speed_selector() -> HtmlSelectElement {
    gloo_utils::document()
        .get_element_by_id(SPEED_SELECTOR_ID)
        .unwrap()
        .dyn_into()
        .unwrap()
}

/// Fetches the playback speed selected in the combobox. Returns 1 if invalid.
fn get_selected_playback_speed() -> f64 {
    let speed_selector = get_speed_selector();
    speed_selector.value().parse().unwrap_or(1.0)
}

/// Sets the playback speed of the <audio> tag and updates the speed selection combobox
fn set_playback_speed(speed: f64, audio_link: &Scope<Audio>) {
    // Set the audio's playback speed
    audio_link.send_message(AudioMsg::SetPlaybackSpeed(speed));

    // If this playback speed appears in the playback speed selector, make it appear selected
    if PLAYBACK_SPEEDS.iter().position(|&s| s == speed).is_some() {
        let speed_selector = get_speed_selector();
        speed_selector.set_value(&format!("{}", speed));
    }
}

/// Gets the elapsed time (the only potentially stale value) and tells the player to save the
/// global state. `periodic` tells the function whether this was called by a timer or by a user
/// action. This is passed on to the player later.
fn trigger_save(periodic: bool, player: &Scope<Player>) {
    // Get the elapsed audio time and send to player state
    let elapsed = GlobalAudio::get_elapsed();
    player.send_message(PlayerMsg::SaveState { elapsed, periodic });
}

#[derive(PartialEq, Properties)]
pub struct Props {
    /// A link to myself. We have to set this on creation
    pub player_link: WeakComponentLink<Player>,
    /// A link to the Queue component
    pub queue_link: WeakComponentLink<Queue>,
}

pub enum PlayerMsg {
    /// Play the given article
    Play(QueueEntry),

    /// Ask the queue for the previous track
    AskForPrevTrack,

    /// Ask the queue for the next track
    AskForNextTrack,

    /// Stops playback if a particular ID is playing. This is so that removing a playing item from
    /// the queue stops the current playback
    StopIfPlaying(ArticleId),

    /// Triggers the Player to check the playback speed selector and update the playback speed
    /// accordingly
    UpdatePlaybackSpeed,

    /// Set the current player state to the one provided. This is used for loading state from the
    /// IndexedDB
    SetState(PlayerState),

    /// A message for the PlayerState to save itself. The only value that's stale is the elapsed
    /// time, so that's given to the PlayerState
    SaveState {
        /// The current article's elapsed time
        elapsed: f64,
        /// Whether this save stems from a periodic save or an ad-hoc save. This determines whether
        /// or not we reset the timer
        periodic: bool,
    },
}

/// Holds the elapsed time in a given article
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArticleState {
    /// The ID of the referenced article
    id: ArticleId,
    /// The elapsed time of the article, in seconds
    elapsed: f64,
}

/// The Player component of our app. This handles all the player logic.
pub struct Player {
    /// A link to the player's <audio> component
    audio_link: WeakComponentLink<Audio>,
    /// The closure that runs every PLAYER_STATE_SAVE_FREQ seconds saving the player state
    _trigger_save_cb: Closure<dyn 'static + Fn()>,
    /// Callbacks for the media session API
    _media_session_cbs: MediaSessionCallbacks,
    /// Holds all the serializable state of this player. This will be loaded from the IndexedDB
    state: PlayerState,
}

/// Holds what's playing, how long it's been playing, and how fast
#[derive(Clone, Serialize, Deserialize)]
pub struct PlayerState {
    /// Handle and title of the currently playing article
    now_playing: Option<QueueEntry>,
    /// The audio playback speed, as a percentage
    playback_speed: f64,
}

impl Default for PlayerState {
    fn default() -> PlayerState {
        PlayerState {
            now_playing: None,
            playback_speed: 1.0,
        }
    }
}

impl Component for Player {
    type Message = PlayerMsg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        // Set the player link to this Player
        ctx.props()
            .player_link
            .borrow_mut()
            .replace(ctx.link().clone());

        // Set up the closure that gets called every 10sec and triggers a save event
        let link = ctx.link().clone();
        let periodic = true;
        let trigger_save_cb = Closure::new(move || trigger_save(periodic, &link));

        // Set up the MediaSession API
        let mut _media_session_cbs = MediaSessionCallbacks::default();
        // Hook up the prev and next track buttons
        let link = ctx.link().clone();
        _media_session_cbs
            .set_prevtrack_action(move || link.send_message(PlayerMsg::AskForPrevTrack));
        let link = ctx.link().clone();
        _media_session_cbs
            .set_nexttrack_action(move || link.send_message(PlayerMsg::AskForNextTrack));

        // Kick off a future to get the last known player state
        let link = ctx.link().clone();
        spawn_local(async move {
            match caching::load_player_state().await {
                Ok(state) => {
                    tracing::trace!("successfully restored player from save");
                    link.send_message(PlayerMsg::SetState(state));
                }
                Err(e) => tracing::error!("Could not load player state: {:?}", e),
            }
        });

        // Kick off the state saving loop in PLAYER_STATE_SAVE_FREQ seconds
        utils::run_after_delay(&trigger_save_cb, PLAYER_STATE_SAVE_FREQ);

        // Return the default values for now. Hopefully they get overwritten by the
        // load_player_state callback
        Self {
            _trigger_save_cb: trigger_save_cb,
            _media_session_cbs,
            state: PlayerState::default(),
            audio_link: WeakComponentLink::default(),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        let audio_link = self.audio_link.borrow().clone().unwrap();
        let queue_link = ctx.props().queue_link.borrow().clone().unwrap();

        match msg {
            PlayerMsg::Play(queue_entry) => {
                let player_link = ctx.link().clone();

                // Change now-playing to the new article
                self.state.now_playing = Some(queue_entry.clone());

                // Load the track, play it, and save the player state to disk
                tracing::debug!("Playing track {}", queue_entry.id.0);
                spawn_local(async move {
                    // Do a useless play() action. This necessary because Safari is buggy and
                    // doesn't allow the first media action (like play or pause) to come from
                    // inside an async worker
                    GlobalAudio::fake_play().await;
                    tracing::trace!("Did a fake play");

                    // Load the article and play it
                    let new_elapsed = prepare_for_play(&queue_entry.id, &audio_link).await;
                    audio_link.send_message(AudioMsg::Play);

                    // Save the new article with the new elapsed time to disk. This isn't done
                    // through trigger_save because it might be the case at this point that the
                    // canplay event has not yet fired on the AudioComponent, so the elapsed time
                    // is 0 instead of the desired elapsed time. So save the new value manually.
                    let periodic = false;
                    player_link.send_message(PlayerMsg::SaveState {
                        elapsed: new_elapsed,
                        periodic,
                    });
                });

                // The state was updated. Refresh the player view
                true
            }

            PlayerMsg::AskForPrevTrack => {
                // Ask the queue to start playing the track that comes before
                // self.state.now_playing
                let now_playing = self.state.now_playing.clone();
                if let Some(ref entry) = now_playing {
                    queue_link.send_message(QueueMsg::PlayTrackBefore(entry.id.clone()))
                }

                false
            }

            PlayerMsg::AskForNextTrack => {
                // Ask the queue to start playing the track that comes after
                // self.state.now_playing
                let now_playing = self.state.now_playing.clone();
                if let Some(ref entry) = &now_playing {
                    queue_link.send_message(QueueMsg::PlayTrackAfter(entry.id.clone()))
                }

                false
            }

            PlayerMsg::UpdatePlaybackSpeed => {
                // Check the playback speed selector and update the playback speed accordingly.
                // Also save the speed in the state.
                let speed = get_selected_playback_speed();
                set_playback_speed(speed, &audio_link);
                self.state.playback_speed = speed;

                // Save state to disk, since it changed. This is an ad-hoc (ie non-periodic) save
                let periodic = false;
                trigger_save(periodic, &ctx.link());

                false
            }

            PlayerMsg::StopIfPlaying(id) => {
                // Check if the given ID matches the currently playing article
                if self.state.now_playing.as_ref().map(|entry| &entry.id) == Some(&id) {
                    // On match, stop playing and clear the <audio> element of all information
                    audio_link.send_message(AudioMsg::Stop);

                    // Now clear the current track, and save the state
                    self.state.now_playing = None;
                    // This is an ad-hoc (ie non-periodic) save
                    let periodic = false;
                    trigger_save(periodic, &ctx.link());

                    // The state was updated. Refresh the player view
                    true
                } else {
                    false
                }
            }

            PlayerMsg::SetState(state) => {
                // Set the state and make it reflected in the player. That is, set the playback
                // speed and the currently playing article
                self.state = state;
                set_playback_speed(self.state.playback_speed, &audio_link);

                // Load up the article specified by now_playing
                if let Some(entry) = self.state.now_playing.clone() {
                    spawn_local(async move {
                        prepare_for_play(&entry.id, &audio_link).await;
                    });
                }

                // The state was updated. Refresh the player view
                true
            }

            PlayerMsg::SaveState { elapsed, periodic } => {
                // If this was a periodic save, set up the next trigger
                if periodic {
                    utils::run_after_delay(&self._trigger_save_cb, PLAYER_STATE_SAVE_FREQ);
                }

                // Sometimes the browser will unload our tab if the audio is paused. When the user
                // comes back to the tab, the page is refreshed and the audio playback is set to
                // 0sec. This is fine, as the user can just hit the Play/Pause button or the queue
                // play button (not the <audio> play button, since that'd play from the beginning).
                // However, if the user does not hit the button within 10sec, the elapsed time of
                // 0sec will be saved, and the user will lose their place. To prevent this, do not
                // do a periodic save if the current elapsed time is 0sec.
                if periodic && elapsed == 0.0 {
                    return false;
                }

                // Collect the states to save. Player state holds now-playing and playback speed.
                // Article state holds elapsed time
                let player_state = self.state.clone();
                let article_state = player_state.now_playing.clone().map(|entry| ArticleState {
                    id: entry.id,
                    elapsed,
                });

                // Save the states
                spawn_local(async move {
                    // Save the player state first
                    match caching::save_player_state(&player_state).await {
                        Ok(_) => tracing::trace!("Successfully saved player state"),
                        Err(e) => tracing::error!("Could not save player state: {:?}", e),
                    }

                    // Try to save the article state. There may well be nothing playing. In which
                    // case, do nothing.
                    if let Some(s) = article_state {
                        match caching::save_article_state(&s).await {
                            Ok(_) => tracing::trace!("Successfully saved article state"),
                            Err(e) => tracing::error!("Could not save article state: {:?}", e),
                        }
                    } else {
                        tracing::trace!("No article to save");
                    }
                });

                false
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let player_link = ctx.props().player_link.borrow().clone().unwrap();

        //
        // Callbacks for the play, jump backward and forward buttons
        //

        let audio_link = self.audio_link.clone();
        let jump_forward_cb = Callback::from(move |_: MouseEvent| {
            audio_link
                .borrow()
                .as_ref()
                .unwrap()
                .send_message(AudioMsg::JumpForward)
        });

        let audio_link = self.audio_link.clone();
        let jump_backward_cb = Callback::from(move |_: MouseEvent| {
            audio_link
                .borrow()
                .as_ref()
                .unwrap()
                .send_message(AudioMsg::JumpBackward)
        });

        let self_link = player_link.clone();
        let now_playing = self.state.now_playing.clone();
        let playpause_cb = Callback::from(move |_: MouseEvent| {
            if GlobalAudio::is_playing() {
                GlobalAudio::pause();
            } else {
                tracing::error!("Now playing = {:?}", &now_playing);
                if let Some(entry) = now_playing.clone() {
                    self_link.send_message(PlayerMsg::Play(entry))
                }
            }
        });

        // Callback for the "go to beginning". When prevtrack is clicked, go to the beginning of
        // the audio. If it's clicked twice within one second, i.e., double-clicked, then move to
        // the previous track.
        let gotobeginning_cb = Callback::from(move |_: MouseEvent| {
            GlobalAudio::seek(0.0);
        });

        // Callback for the playback speed
        let playback_speed_cb = player_link.callback(|_| PlayerMsg::UpdatePlaybackSpeed);

        // Set nowplaying
        let now_playing = self.state.now_playing.clone();
        let playback_speed_selector = render_playback_speed_selector(playback_speed_cb);
        let now_playing_html = now_playing
            .as_ref()
            .map(|entry| html! {<span> {entry.title.clone()} </span>})
            .unwrap_or(html! {<span style="font-style: italic">{"[no article loaded]"}</span>});

        let audio_link = self.audio_link.clone();
        html! {
            <section title="Player">
                <h2>{ "Player" }</h2>
                <p><strong>{ "Now Playing: " }</strong> { now_playing_html }</p>
                <Audio {audio_link} />
                <div class="audiocontrol" title="More playback controls">
                    <button
                        aria-label="Go to beginning"
                        title="Go to beginning"
                        onclick={gotobeginning_cb}
                    >
                        { "⏮️" }
                    </button>

                    <button
                        aria-label="Jump backwards 10 seconds"
                        title="Jump backwards 10 seconds"
                        onclick={jump_backward_cb}
                    >
                        { "↩️" }
                    </button>
                    <button
                        aria-label="Jump forwards 10 seconds"
                        title="Jump forwards 10 seconds"
                        onclick={jump_forward_cb}
                    >
                    { "↪️" }
                    </button>

                    <div class="playbackSpeedSection">
                        <label id="speedSelectorLabel" for={SPEED_SELECTOR_ID}>
                            { "Playback Speed:" }
                        </label>
                        { playback_speed_selector }
                    </div>
                </div>
            </section>
        }
    }
}

/// Renders the playback speed selector, using the given callback for onchange events
fn render_playback_speed_selector(onchange: Callback<Event>) -> Html {
    // Construct all the <option> values
    let options: Html = PLAYBACK_SPEEDS
        .iter()
        .map(|speed| {
            let speed_str = format!("{}", speed);
            // Make the option. The 1.0 speed is selected by default.
            html! {
                <option value={ speed_str.clone() } selected={*speed == 1.0}>
                    { speed_str }
                </option>
            }
        })
        .collect();

    html! {
        <select title="Playback speed" name={SPEED_SELECTOR_ID} id={SPEED_SELECTOR_ID} onchange={onchange}>
            { options }
        </select>
    }
}
