use crate::{
    caching,
    player_view::{Player, PlayerMsg},
    WeakComponentLink,
};

use serde::{Deserialize, Serialize};
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

#[derive(PartialEq, Properties)]
pub struct Props {
    /// A link to myself. We have to set this on creation
    pub queue_link: WeakComponentLink<Queue>,
    /// A link to the Player component
    pub player_link: WeakComponentLink<Player>,
}

/// An entry in the queue has the title and ID of the article
#[derive(Clone, Serialize, Deserialize)]
pub struct QueueEntry {
    pub(crate) id: ArticleId,
    pub(crate) title: String,
}

pub enum QueueMsg {
    /// Adds the given entry to the queue
    Add(QueueEntry),
    /// Deletes the entry at the given index
    Delete(usize),
    /// Sets the queue contents. Used in loading from previous state
    SetQueue(Queue),
    /// A message from the player asking to get the article that comes after the given one
    PlayTrackAfter(ArticleId),
    /// A message from the player asking to get the article that comes before the given one
    PlayTrackBefore(ArticleId),
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CachedArticle {
    pub title: String,
    // TODO: Make id unique. Currently it's just a copy of the title
    pub id: ArticleId,
    pub audio_blob: Vec<u8>,
}

impl From<&CachedArticle> for QueueEntry {
    fn from(article: &CachedArticle) -> QueueEntry {
        QueueEntry {
            title: article.title.clone(),
            id: article.id.clone(),
        }
    }
}

/// A handle to retrieve a cached article from storage. This is just the title for now
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArticleId(pub(crate) String);

#[derive(Clone, Serialize, Deserialize)]
pub struct QueuePosition {
    /// Title of article currently playing
    cur_article: ArticleId,
    /// Current timestamp in the playback
    cur_timestamp: f64,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Queue {
    entries: Vec<QueueEntry>,
}

impl Queue {
    /// Saves the queue to the IndexedDB
    fn save(&self) {
        let self_copy = self.clone();
        spawn_local(async move {
            let _ = caching::save_queue(&self_copy)
                .await
                .map_err(|e| tracing::error!("Couldn't restore queue: {:?}", e));
        });
    }

    /// Attempts to load the queue from IndexedDB
    async fn load() -> Option<Queue> {
        caching::load_queue()
            .await
            .map_err(|e| tracing::error!("Couldn't restore queue: {:?}", e))
            .ok()
    }
}

impl Component for Queue {
    type Message = QueueMsg;
    type Properties = Props;

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        let player_link = ctx.props().player_link.borrow().clone().unwrap();
        match msg {
            QueueMsg::Delete(idx) => {
                // Remove the entry from the queue and delete the article from the cache
                let entry = self.entries.remove(idx);

                // Tell the player to stop playing this track if it's playing
                player_link.send_message(PlayerMsg::StopIfPlaying(entry.id.clone()));

                // Save the queue
                self.save();

                // Delete the article from storage
                spawn_local(async move {
                    // Delete the article itself
                    let _ = caching::delete_article(&entry.id).await.map_err(|e| {
                        tracing::error!("Couldn't delete article {}: {:?}", &entry.id.0, e)
                    });

                    // Delete the reader's position in the article
                    let _ = caching::delete_article_state(&entry.id).await.map_err(|e| {
                        tracing::error!("Couldn't delete state of article {}: {:?}", &entry.id.0, e)
                    });
                });
            }
            QueueMsg::Add(entry) => {
                // Add the entry to the queue
                self.entries.push(entry);
                // Save it to IndexedDB
                self.save()
            }
            QueueMsg::SetQueue(queue) => {
                *self = queue;
            }
            QueueMsg::PlayTrackBefore(article_id) => {
                // Find the article ID in the queue
                let now_playing_idx = self.entries.iter().position(|x| x.id == article_id);
                // Get the ID of the next track, if it exist
                let next_track_id = now_playing_idx
                    .and_then(|i| self.entries.get(i + 1))
                    .map(|e| e.id.clone());

                // If the ID exists, play it
                if let Some(id) = next_track_id {
                    player_link.send_message(PlayerMsg::Play(id.clone()));
                }
            }
            QueueMsg::PlayTrackAfter(article_id) => {
                // Find the article ID in the queue
                let now_playing_idx = self.entries.iter().position(|x| x.id == article_id);
                // Get the ID of the prev track, if it exist
                let prev_track_id = now_playing_idx
                    .and_then(|i| i.checked_sub(1))
                    .and_then(|i| self.entries.get(i))
                    .map(|e| e.id.clone());

                // If the ID exists, play it
                if let Some(id) = prev_track_id {
                    player_link.send_message(PlayerMsg::Play(id.clone()));
                }
            }
        }

        true
    }

    fn create(ctx: &Context<Self>) -> Self {
        // Set the queue link to this Queue
        ctx.props()
            .queue_link
            .borrow_mut()
            .replace(ctx.link().clone());

        // Kick off a future to load the saved queue from IndexedDB
        ctx.link().send_future_batch(async move {
            // load() returns an Option<Queue>. Turn it into a singleton or empty vec, and make it
            // a SetQueue message
            Queue::load()
                .await
                .into_iter()
                .map(QueueMsg::SetQueue)
                .collect()
        });

        Self::default()
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let player_link = &ctx.props().player_link;
        let queue_link = &ctx.props().queue_link;

        let rendered_list = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, entry)| render_queue_item(entry, i, player_link, queue_link))
            .collect::<Html>();
        html! {
            <section title="Queue">
                <h2>{ "Queue" }</h2>
                <ul role="list" aria-label="Queue entries">
                    { rendered_list }
                </ul>
            </section>
        }
    }
}

fn render_queue_item(
    entry: &QueueEntry,
    pos: usize,
    player_link: &WeakComponentLink<Player>,
    queue_link: &WeakComponentLink<Queue>,
) -> Html {
    let player_scope = player_link.borrow().clone().unwrap();
    let queue_scope = queue_link.borrow().clone().unwrap();
    let id = entry.id.clone();

    let play_callback = Callback::from(move |_| {
        player_scope.send_message(PlayerMsg::Play(id.clone()));
    });
    let remove_callback = queue_scope.callback(move |_| QueueMsg::Delete(pos));

    let play_title_text = format!("Play: {}", entry.title);
    let delete_title_text = format!("Delete from queue: {}", entry.title);

    html! {
        <li aria-label={ entry.title.clone() } class="queueControl">
            <button
                class="queuePlay"
                aria-label={ play_title_text.clone() }
                title={ play_title_text }
                onclick={play_callback}
            >
                { "▶️" }
            </button>
            <p aria-hidden="true" class="queueArticleTitle"> {&entry.title} </p>
            <button
                class="queueDelete"
                aria-label={ delete_title_text.clone() }
                title={ delete_title_text }
                onclick={remove_callback}
            >
                { "🗑" }
            </button>
        </li>
    }
}
