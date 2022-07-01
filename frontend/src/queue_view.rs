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

pub enum QueueMsg {
    AddHandle(CachedArticleHandle),
    Delete(usize),
    LoadFrom(Vec<CachedArticleHandle>),
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CachedArticle {
    pub title: String,
    pub audio_blob: Vec<u8>,
}

/// A handle to retrieve a cached article from storage. This is just the title for now
#[derive(Clone)]
pub struct CachedArticleHandle(pub(crate) String);

pub struct QueuePosition {
    /// Title of article currently playing
    cur_article: CachedArticleHandle,
    /// Current timestamp in the playback
    cur_timestamp: f64,
}

#[derive(Default)]
pub struct Queue {
    article_handles: Vec<CachedArticleHandle>,
    cur_pos: Option<QueuePosition>,
}

impl Component for Queue {
    type Message = QueueMsg;
    type Properties = Props;

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            QueueMsg::Delete(idx) => {
                // Remove the handle from the queue and delete it from the cache
                let handle = self.article_handles.remove(idx);
                spawn_local(async move {
                    // Try deleting
                    match caching::delete_article(&handle).await {
                        Err(e) => tracing::error!("{:?}", e),
                        _ => (),
                    }
                });
            }
            QueueMsg::AddHandle(handle) => self.article_handles.push(handle),
            QueueMsg::LoadFrom(handles) => {
                self.article_handles = handles;
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
            match caching::load_handles().await {
                Ok(handles) => link.send_message(QueueMsg::LoadFrom(handles)),
                Err(e) => tracing::error!("{:?}", e),
            }
        });

        Self::default()
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let player_link = &ctx.props().player_link;
        let queue_link = &ctx.props().queue_link;

        let rendered_list = self
            .article_handles
            .iter()
            .enumerate()
            .map(|(i, handle)| render_queue_item(handle, i, player_link, queue_link))
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
    handle: &CachedArticleHandle,
    pos: usize,
    player_link: &WeakComponentLink<Player>,
    queue_link: &WeakComponentLink<Queue>,
) -> Html {
    let player_scope = player_link.borrow().clone().unwrap();
    let queue_scope = queue_link.borrow().clone().unwrap();
    let handle_copy = handle.clone();

    let play_callback = Callback::from(move |_| {
        player_scope.send_message(PlayerMsg::PlayHandle(handle_copy.clone()));
    });
    let remove_callback = queue_scope.callback(move |_| QueueMsg::Delete(pos));
    html! {
        <li>
            <button title="Delete from queue" onclick={remove_callback}>{ "🗑" }</button>
            <button title="Play" onclick={play_callback}>{ "▶️" }</button>
            <p> {&handle.0} </p>
        </li>
    }
}
