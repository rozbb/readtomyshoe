use crate::{
    caching,
    player_view::{Player, PlayerMsg},
    WeakComponentLink,
};

use common::ArticleList;

use gloo_net::http::Request;
use serde::{Deserialize, Serialize};
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;
use yew_router::prelude::*;

#[derive(PartialEq, Properties)]
pub struct Props {
    /// A link to myself. We have to set this on creation
    pub queue_link: WeakComponentLink<Queue>,
    /// A link to the Player component
    pub player_link: WeakComponentLink<Player>,
}

pub enum QueueMsg {
    Add(CachedArticle),
    AddHandle(CachedArticleHandle),
    LoadFrom(Vec<CachedArticleHandle>),
    Remove(usize),
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
    cur_article: String,
    /// Current timestamp in the playback
    cur_timestamp: f64,
}

#[derive(Default)]
pub struct Queue {
    articles: Vec<CachedArticle>,
    article_handles: Vec<CachedArticleHandle>,
    cur_pos: usize,
}

impl Component for Queue {
    type Message = QueueMsg;
    type Properties = Props;

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            QueueMsg::Add(article) => self.articles.push(article),
            QueueMsg::AddHandle(handle) => self.article_handles.push(handle),
            QueueMsg::LoadFrom(handles) => {
                self.article_handles = handles;
            }
            QueueMsg::Remove(idx) => {
                self.articles.remove(idx);
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
        self.article_handles
            .iter()
            .map(|handle| {
                let player_link = ctx.props().player_link.borrow().clone().unwrap();
                let handle_copy = handle.clone();
                let callback = Callback::from(move |_| {
                    player_link.send_message(PlayerMsg::PlayHandle(handle_copy.clone()));
                });
                html! {
                    <p>
                        {&handle.0}
                        <button onclick={callback}>{ "▶️" }</button>
                    </p>
                }
            })
            .collect::<Html>()
    }
}
