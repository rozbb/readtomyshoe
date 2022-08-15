use crate::{
    app_view::Route,
    caching,
    queue_view::{ArticleId, CachedArticle, Queue, QueueEntry, QueueMsg},
    WeakComponentLink,
};
use common::{ArticleMetadata, LibraryCatalog};

use anyhow::{bail, Error as AnyError};
use gloo_net::http::Request;
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use web_sys::PageTransitionEvent;
use yew::{html::Scope, prelude::*};
use yew_router::prelude::*;

/// Fetches the list of articles
async fn fetch_article_list() -> Result<LibraryCatalog, AnyError> {
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
pub async fn fetch_article(id: &str, title: &str) -> Result<CachedArticle, AnyError> {
    // Fetch the audio blobs
    let filename = format!("{id}.mp3");
    let encoded_title = urlencoding::encode(&filename);
    let resp = Request::get(&format!("/api/audio-blobs/{encoded_title}"))
        .send()
        .await
        .map_err(|e| {
            let ctx = format!("Error fetching article {id}");
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
        id: ArticleId(id.to_string()),
        audio_blob,
    })
}

/// Callback for "pageshow" event. Gets triggered whenever this page gets shown due to navigation
fn pageshow_callback(event: PageTransitionEvent, library_link: Scope<Library>) {
    // If the page is loading from a cache, force it to reload the library
    if event.persisted() {
        library_link.send_message(LibraryMsg::FetchLibrary);
    }
}

/// Renders an item in the library
fn render_lib_item(metadata: ArticleMetadata, library_link: Scope<Library>) -> Html {
    let title = metadata.title.clone();
    let id = metadata.id.clone();
    let title_copy = title.clone();
    let callback = library_link.callback(move |_| LibraryMsg::FetchArticle {
        id: id.clone(),
        title: title_copy.clone(),
    });

    // Format the date the article was added
    let lang = gloo_utils::window()
        .navigator()
        .language()
        .unwrap_or("en-US".to_string());
    let date_added: Option<String> = metadata.datetime_added.map(|t| {
        // Convert to a local date string  by making a Date object and giving it the unix time
        let js_date = js_sys::Date::new_0();
        // set_time takes number of milliseconds since epoch, so multiply by 1000
        js_date.set_time((t as f64) * 1000.0);
        js_date.to_locale_string(&lang, &JsValue::TRUE).into()
    });
    let date_added_str = match date_added {
        Some(d) => format!("Added on {d}"),
        None => format!("Date added unknown"),
    };
    let add_title_text = format!("Add to queue: {}", title);

    html! {
        <li aria-label={ title.clone() }>
            <button
                onclick={callback}
                class="addToQueue"
                aria-label={ add_title_text.clone() }
                title={ add_title_text }
            >
                { "+" }
            </button>
            <div class="articleDetails">
                <p aria-hidden="true" class="libArticleTitle">{ title }</p>
                <p class="dateAdded">{ date_added_str }</p>
            </div>
        </li>
    }
}

#[derive(Default)]
pub(crate) struct Library {
    err: Option<AnyError>,
    catalog: Option<LibraryCatalog>,
    _pageshow_action: Option<Closure<dyn 'static + Fn(PageTransitionEvent)>>,
}

pub(crate) enum LibraryMsg {
    SetCatalog(LibraryCatalog),
    SetError(AnyError),
    FetchArticle { id: String, title: String },
    FetchLibrary,
    PassArticleToQueue(QueueEntry),
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
            LibraryMsg::SetCatalog(catalog) => {
                self.err = None;
                self.catalog = Some(catalog);
            }
            LibraryMsg::SetError(err) => {
                self.catalog = None;
                self.err = Some(err);
            }
            LibraryMsg::FetchLibrary => {
                ctx.link().send_future(async move {
                    match fetch_article_list().await {
                        Ok(list) => LibraryMsg::SetCatalog(list),
                        Err(e) => LibraryMsg::SetError(e.into()),
                    }
                });
            }
            LibraryMsg::FetchArticle { id, title } => {
                // Fetch an article, save it, and relay the article handle. If there's an error,
                // post it
                ctx.link()
                    .callback_future_once(|()| async move {
                        let article = match fetch_article(&id, &title).await {
                            Ok(a) => a,
                            Err(e) => return LibraryMsg::SetError(e),
                        };

                        let queue_entry = match caching::save_article(&article).await {
                            Ok(h) => h,
                            Err(e) => return LibraryMsg::SetError(e),
                        };

                        LibraryMsg::PassArticleToQueue(queue_entry)
                    })
                    .emit(());
            }
            LibraryMsg::PassArticleToQueue(queue_entry) => {
                // If we saved an article, pass the notif to the queue
                ctx.props()
                    .queue_link
                    .borrow()
                    .clone()
                    .unwrap()
                    .send_message(QueueMsg::Add(queue_entry));
            }
        }

        true
    }

    fn create(ctx: &Context<Self>) -> Self {
        // Kick of a future that will fetch the article list
        ctx.link().send_future(async move {
            match fetch_article_list().await {
                Ok(list) => LibraryMsg::SetCatalog(list),
                Err(e) => LibraryMsg::SetError(e.into()),
            }
        });

        // Save the pageshow callback and set it on document.window
        let link = ctx.link().clone();
        let pageshow_cb = Closure::new(move |evt| pageshow_callback(evt, link.clone()));
        gloo_utils::window()
            .add_event_listener_with_callback("pageshow", pageshow_cb.as_ref().unchecked_ref())
            .expect("couldn't register pageshow callback");

        Library {
            _pageshow_action: Some(pageshow_cb),
            ..Default::default()
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        // If there's an error, render it
        if let Some(err) = &self.err {
            html! {
                <p style={ "color: red;" } aria-live="assertive" aria-role="alert" title="errors">
                    { format!("{:?}", err) }
                </p>
            }
        } else if let Some(catalog) = &self.catalog {
            // If there's a list, render all the items
            let rendered_list = catalog
                .0
                .iter()
                .map(|metadata| render_lib_item(metadata.clone(), ctx.link().clone()))
                .collect::<Html>();
            html! {
                <section title="Library">
                    <h2>{ "Library" }</h2>
                    <div id="addArticle">
                        <Link<Route> to={Route::Add} classes="navLink">
                            { "Add Article" }
                        </Link<Route>>
                    </div>
                    <ul role="list" aria-label="Library catalog">
                        { rendered_list }
                    </ul>
                <p id="libErrors" style={ "color: red;" } aria-live="assertive" aria-role="alert" title="errors">
                </p>
                </section>
            }
        } else {
            Default::default()
        }
    }
}
