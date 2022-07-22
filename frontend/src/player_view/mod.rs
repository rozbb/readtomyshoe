mod audio_component;

use crate::{caching, queue_view::ArticleId, utils, WeakComponentLink};
use audio_component::{Audio, AudioMsg, GlobalAudio, MediaSessionState};

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

/// Loads the given article and its playback state, and sets the <audio>'s src to the MP3 blob
async fn prepare_for_play(id: &ArticleId, audio_link: &Scope<Audio>) {
    // Load the article state and set the elapsed time.
    let elapsed = match caching::load_article_state(&id).await {
        Ok(state) => state.elapsed,
        Err(e) => {
            tracing::warn!("Couldn't load article state {}: {:?}", id.0, e);
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
            return;
        }
    }
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
    // Update the MediaSession state while we're here. This should lessen discontinuities due to
    // difference in true playback speed and displayed playback speed.
    MediaSessionState::update();

    // Get the elapsed audio time and send to player state
    let elapsed = GlobalAudio::get_elapsed();
    player.send_message(PlayerMsg::SaveState { elapsed, periodic });
}

#[derive(PartialEq, Properties)]
pub struct Props {
    /// A link to myself. We have to set this on creation
    pub player_link: WeakComponentLink<Player>,
}

pub enum PlayerMsg {
    /// Play the given article
    Play(ArticleId),

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
#[derive(Clone, Serialize, Deserialize)]
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
    /// Holds all the serializable state of this player. This will be loaded from the IndexedDB
    state: PlayerState,
}

/// Holds what's playing, how long it's been playing, and how fast
#[derive(Clone, Serialize, Deserialize)]
pub struct PlayerState {
    /// Handle of the currently playing article
    now_playing: Option<ArticleId>,
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

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        let audio_link = self.audio_link.borrow().clone().unwrap();

        match msg {
            PlayerMsg::Play(id) => {
                let player_link = ctx.link().clone();

                // Change now-playing to the new article
                self.state.now_playing = Some(id.clone());

                // Load the track, play it, and save the player state to disk
                tracing::debug!("Playing track {}", id.0);
                spawn_local(async move {
                    // Do a useless play() action. This necessary because Safari is buggy and
                    // doesn't allow the first media action (like play or pause) to come from
                    // inside an async worker
                    GlobalAudio::fake_play().await;
                    tracing::trace!("Did a fake play");

                    // Load the article and play it
                    prepare_for_play(&id, &audio_link).await;
                    audio_link.send_message(AudioMsg::Play);

                    // Save the state to disk. This is an ad-hoc (ie non-periodic) save
                    let periodic = false;
                    trigger_save(periodic, &player_link);
                });

                // The state was updated. Refresh the player view
                true
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
                if self.state.now_playing == Some(id) {
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
                if let Some(handle) = self.state.now_playing.clone() {
                    spawn_local(async move { prepare_for_play(&handle, &audio_link).await });
                }

                // The state was updated. Refresh the player view
                true
            }

            PlayerMsg::SaveState { elapsed, periodic } => {
                // Collect the states to save. Player state holds now-playing and playback speed.
                // Article state holds elapsed time
                let player_state = self.state.clone();
                let article_state = player_state
                    .now_playing
                    .clone()
                    .map(|id| ArticleState { id, elapsed });

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

                // If this was a periodic save, set up the next trigger
                if periodic {
                    utils::run_after_delay(&self._trigger_save_cb, PLAYER_STATE_SAVE_FREQ);
                }

                false
            }
        }
    }

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

        // Kick off a future to get the last known player state
        let link = ctx.link().clone();
        spawn_local(async move {
            match caching::load_player_state().await {
                Ok(state) => {
                    tracing::trace!("successfully restored player from save");
                    link.send_message(PlayerMsg::SetState(state));
                }
                Err(e) => tracing::error!("could not load player state: {:?}", e),
            }
        });

        // Kick off the state saving loop in PLAYER_STATE_SAVE_FREQ seconds
        utils::run_after_delay(&trigger_save_cb, PLAYER_STATE_SAVE_FREQ);

        // Return the default values for now. Hopefully they get overwritten by the
        // load_player_state callback
        Self {
            _trigger_save_cb: trigger_save_cb,
            state: PlayerState::default(),
            audio_link: WeakComponentLink::default(),
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let player_link = &ctx.props().player_link;

        // Callbacks for the left and right arrow buttons
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
        // Callback for the playback speed
        let playback_speed_cb = player_link
            .borrow()
            .as_ref()
            .unwrap()
            .callback(|_| PlayerMsg::UpdatePlaybackSpeed);

        let playback_speed_selector = render_playback_speed_selector(playback_speed_cb);
        let now_playing_str = self
            .state
            .now_playing
            .as_ref()
            .map(|c| c.0.clone())
            .unwrap_or(String::default());

        let audio_link = self.audio_link.clone();
        html! {
            <section title="player">
                <p><b>{ "Now Playing: " }</b> { now_playing_str }</p>
                <Audio {audio_link} />
                <div class="audiocontrol" title="More playback controls">
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
