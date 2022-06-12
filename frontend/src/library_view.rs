use crate::{
    caching,
    queue_view::{CachedArticle, CachedArticleHandle, Queue, QueueMsg},
    WeakComponentLink,
};
use common::ArticleList;

use anyhow::{bail, Error as AnyError};
use gloo_net::http::Request;
use yew::{html::Scope, prelude::*};

/// Fetches the list of articles
async fn fetch_article_list() -> Result<ArticleList, AnyError> {
    tracing::debug!("Fetching article list");
    let resp = Request::get("/api/list-articles")
        .send()
        .await
        .map_err(|e| AnyError::from(e).context("Error fetching article list"))?;
    tracing::debug!("Done fetching. Response is {:?}", resp);

    if !resp.ok() {
        tracing::debug!("Bailing");
        bail!(
            "Error fetching article list {} ({})",
            resp.status(),
            resp.status_text()
        );
    }

    resp.json()
        .await
        .map_err(|e| AnyError::from(e).context("Error parsing article list JSON"))
}

/// Fetches a specific article
pub async fn fetch_article(title: &str) -> Result<CachedArticle, AnyError> {
    // Fetch the audio blobs
    let resp = Request::get(&format!("/api/audio-blobs/{title}"))
        .send()
        .await
        .map_err(|e| {
            let ctx = format!("Error fetching article {title}");
            AnyError::from(e).context(ctx)
        })?;
    if !resp.ok() {
        bail!(
            "Error fetching article {} ({})",
            resp.status(),
            resp.status_text()
        );
    }

    tracing::debug!("Got audio blob {:?}", resp);

    // Parse the binary
    let audio_blob = resp
        .binary()
        .await
        .map_err(|e| AnyError::msg(format!("Error parsing audio binary: {e}")))?;

    // Return the article
    Ok(CachedArticle {
        title: title.to_string(),
        audio_blob,
    })
}

/// Renders an item in the library
fn render_lib_item(title: String, library_link: Scope<Library>) -> Html {
    //let callback = make_article_fetch_callback(title.clone(), queue_link);
    let title_copy = title.clone();
    let callback = library_link.callback(move |_| LibraryMsg::FetchArticle(title_copy.clone()));

    html! {
        <span>
            <p>{title}
                <button onclick={callback}>{ "+" }</button>
            </p>
        </span>
    }
}

#[derive(Default)]
pub(crate) struct Library {
    err: Option<AnyError>,
    list: Option<ArticleList>,
}

pub(crate) enum LibraryMsg {
    SetList(ArticleList),
    SetError(AnyError),
    FetchArticle(String),
    PassArticleToQueue(CachedArticleHandle),
}

#[derive(PartialEq, Properties)]
pub struct Props {
    pub queue_link: WeakComponentLink<Queue>,
}

impl Component for Library {
    type Message = LibraryMsg;
    type Properties = Props;

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            LibraryMsg::SetList(list) => {
                self.err = None;
                self.list = Some(list);
            }
            LibraryMsg::SetError(err) => {
                self.list = None;
                self.err = Some(err);
            }
            LibraryMsg::FetchArticle(title) => {
                // Fetch an article, save it, and relay the article handle. If there's an error,
                // post it
                ctx.link()
                    .callback_future_once(|()| async move {
                        let article = match fetch_article(&title).await {
                            Ok(a) => a,
                            Err(e) => return LibraryMsg::SetError(e),
                        };

                        let handle = match caching::save_article(&article).await {
                            Ok(h) => h,
                            Err(e) => return LibraryMsg::SetError(e),
                        };

                        LibraryMsg::PassArticleToQueue(handle)
                    })
                    .emit(());
            }
            LibraryMsg::PassArticleToQueue(handle) => {
                // If we saved an article, pass the notif to the queue
                ctx.props()
                    .queue_link
                    .borrow()
                    .clone()
                    .unwrap()
                    .send_message(QueueMsg::AddHandle(handle));
            }
        }

        true
    }

    fn create(ctx: &Context<Self>) -> Self {
        // Kick of a future that will fetch the article list
        ctx.link()
            .callback_future_once(|()| async move {
                tracing::info!("Calling fetch_article_list");
                match fetch_article_list().await {
                    Ok(list) => LibraryMsg::SetList(list),
                    Err(e) => LibraryMsg::SetError(e.into()),
                }
            })
            .emit(());

        Self::default()
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        // If there's an error, render it
        if let Some(err) = &self.err {
            html! {
                <p style={ "color: red;" }>
                    { format!("{:?}", err) }
                </p>
            }
        } else if let Some(list) = &self.list {
            // If there's a list, render all the items
            list.titles
                .iter()
                .map(|title| render_lib_item(title.clone(), ctx.link().clone()))
                .collect::<Html>()
        } else {
            Default::default()
        }
    }
}
