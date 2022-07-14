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
pub struct QueueEntry {
    pub(crate) id: ArticleId,
    pub(crate) title: String,
}

pub enum QueueMsg {
    Add(QueueEntry),
    Delete(usize),
    LoadFrom(Vec<QueueEntry>),
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
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArticleId(pub(crate) String);

#[derive(Clone, Serialize, Deserialize)]
pub struct QueuePosition {
    /// Title of article currently playing
    cur_article: ArticleId,
    /// Current timestamp in the playback
    cur_timestamp: f64,
}

#[derive(Default)]
pub struct Queue {
    entries: Vec<QueueEntry>,
}

impl Component for Queue {
    type Message = QueueMsg;
    type Properties = Props;

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            QueueMsg::Delete(idx) => {
                // Remove the entry from the queue and delete the article from the cache
                let entry = self.entries.remove(idx);

                // Tell the player to stop playing this track if it's playing
                ctx.props()
                    .player_link
                    .borrow()
                    .clone()
                    .unwrap()
                    .send_message(PlayerMsg::StopIfPlaying(entry.id.clone()));

                // Delete the article from storage
                spawn_local(async move {
                    // Delete the article itself
                    match caching::delete_article(&entry.id).await {
                        Err(e) => {
                            tracing::error!("Couldn't delete article {}: {:?}", &entry.id.0, e)
                        }
                        _ => (),
                    }

                    // Delete the reader's position in the article
                    match caching::delete_article_state(&entry.id).await {
                        Err(e) => {
                            tracing::error!(
                                "Couldn't delete state of article {}: {:?}",
                                &entry.id.0,
                                e
                            )
                        }
                        _ => (),
                    }
                });
            }
            QueueMsg::Add(entry) => self.entries.push(entry),
            QueueMsg::LoadFrom(entries) => {
                self.entries = entries;
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

        // Try to get the cached queue from the IndexedDB
        let link = ctx.link().clone();
        spawn_local(async move {
            match caching::load_queue_entries().await {
                Ok(entries) => link.send_message(QueueMsg::LoadFrom(entries)),
                Err(e) => tracing::error!("Couldn't restore queue: {:?}", e),
            }
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
            <section title="queue">
                <ul>
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
    html! {
        <li class="queueControl">
            <button class="queueControlPlay" title="Play" onclick={play_callback}>{ "▶️" }</button>
            <p class="articleTitle"> {&entry.title} </p>
            <button class="queueControlDelete" title="Delete from queue" onclick={remove_callback}>{ "🗑" }</button>
        </li>
    }
}
