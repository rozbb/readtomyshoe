use crate::{
    app_view::Route,
    caching,
    queue_view::{ArticleId, CachedArticle, Queue, QueueEntry, QueueMsg},
    WeakComponentLink,
};
use common::{ArticleMetadata, LibraryCatalog};

use std::collections::BTreeMap;

use anyhow::{bail, Error as AnyError};
use gloo_net::http::Request;
use url::Url;
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use web_sys::PageTransitionEvent;
use yew::{html::Scope, prelude::*};
use yew_router::prelude::*;

/// Fetches the list of articles
async fn fetch_catalog() -> Result<LibraryCatalog, AnyError> {
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

/// Fetches a specific article. The `lib_link` parameter is so it can report fetch progress.
async fn fetch_article(
    id: &ArticleId,
    title: &str,
    lib_link: Scope<Library>,
) -> Result<CachedArticle, AnyError> {
    // Fetch the audio blobs
    let filename = format!("{}.mp3", id.0);
    let encoded_title = urlencoding::encode(&filename);
    let resp = Request::get(&format!("/api/audio-blobs/{encoded_title}"))
        .send()
        .await
        .map_err(|e| {
            let ctx = format!("Error fetching article {:?}", id);
            AnyError::from(e).context(ctx)
        })?;
    if !resp.ok() {
        bail!(
            "Error fetching article {} ({})",
            resp.status(),
            resp.status_text()
        );
    }

    // Get the size of the audio blob. This is the denominator when we compute percentage
    // downloaded.
    let blob_size = resp
        .headers()
        .get("content-length")
        .unwrap()
        .parse::<usize>()
        .unwrap();

    // Convert the response into one that will fire a given callback every time it gets a chunk. We
    // use this callback to inform the library of the download status.
    let mut num_bytes_read = 0;
    let id_copy = id.clone();
    let resp = crate::utils::response_with_read_callback(resp, move |chunk| {
        // Update the number of bytes read
        num_bytes_read += chunk.byte_length() as usize;

        // Tell the library how much progress we've made
        lib_link.send_message(LibraryMsg::SetDownloadProgress {
            id: id_copy.clone(),
            progress: num_bytes_read as f64 / blob_size as f64,
        });
    })?;

    // Download the whole response
    let audio_blob = resp
        .binary()
        .await
        .map_err(|e| AnyError::msg(format!("Error parsing audio binary: {e}")))?;

    // Return the article
    Ok(CachedArticle {
        title: title.to_string(),
        id: id.clone(),
        audio_blob,
    })
}

/// Renders an item in the library
fn render_lib_item(
    metadata: ArticleMetadata,
    library_link: Scope<Library>,
    download_progress: Option<DownloadProgress>,
) -> Html {
    let title = metadata.title.clone();
    let id = ArticleId(metadata.id.clone());
    let title_copy = title.clone();

    let callback = library_link.callback(move |_| LibraryMsg::FetchArticle {
        id: id.clone(),
        title: title_copy.clone(),
    });

    // Make a source URL link if it exists and is valid. Otherwise make this part empty.
    let url = metadata
        .source_url
        .as_ref()
        .and_then(|u| Url::parse(u).ok())
        .map(|u| html! { <a href={ String::from(u) } title="Article source">{ "[source]" }</a> })
        .unwrap_or(Html::default());

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
        Some(d) => format!("Added {d}"),
        None => format!("Date added unknown"),
    };

    // If the article is downloading, display download progress instead of the "Add to Queue"
    // button
    let elem_id = format!("lib-{}", metadata.id);

    let add_to_queue_button = if let Some(progress) = download_progress {
        match progress {
            DownloadProgress::InProgress(fraction) => {
                let pct = format!("{}%", (100.0 * fraction).floor() as usize);
                html! {
                    <button
                        class="percentage"
                        id={elem_id}
                        aria-label={ pct.clone() }
                        title={ pct.clone() }
                        disabled=true
                    >
                        { pct }
                    </button>
                }
            }
            DownloadProgress::Done => {
                let add_title_text = format!("Downloaded {title}");
                html! {
                    <button
                        class="downloadDone"
                        id={elem_id}
                        aria-label={ add_title_text.clone() }
                        title={ add_title_text }
                        disabled=true
                    >
                        { "✔︎" }
                    </button>
                }
            }
        }
    } else {
        let add_title_text = format!("Add to queue: {}", title);
        html! {
            <button
                id={elem_id}
                onclick={callback}
                aria-label={ add_title_text.clone() }
                title={ add_title_text }
            >
                { "+" }
            </button>
        }
    };

    html! {
        <tr role="listitem" aria-label={ title.clone() }>
            <td class="addToQueue">{add_to_queue_button}</td>
            <td class = "articleDetails">
                <p class="libArticleTitle">{ title }</p>
                <span class="articleMetadata">{ date_added_str }</span>
                <span class="articleMetadata">{ url }</span>
            </td>
        </tr>
    }
}

/// Describes whether an article is downloading (and if so, how much of it has downloaded), or if
/// it's done downloading
#[derive(Copy, Clone, Debug)]
enum DownloadProgress {
    InProgress(f64),
    Done,
}

// The main state of the Library view
#[derive(Default)]
pub(crate) struct Library {
    err: Option<AnyError>,
    catalog: Option<LibraryCatalog>,
    download_progresses: BTreeMap<ArticleId, DownloadProgress>,
    _pageshow_action: Option<Closure<dyn 'static + Fn(PageTransitionEvent)>>,
}

pub(crate) enum LibraryMsg {
    /// Sets the Library catalog to the given value
    SetCatalog(LibraryCatalog),
    /// Sets the Library's error display to the given error
    SetError(AnyError),
    /// Tells the library to do a fetch() for the specific article
    FetchArticle { id: ArticleId, title: String },
    /// Tells the library to fetch() the catalog
    FetchCatalog,
    /// Updates the download progress of the given article
    SetDownloadProgress { id: ArticleId, progress: f64 },
    /// Tells the Library to send the given queue entry to the Queue
    PassArticleToQueue(QueueEntry),
    /// Sets the given articles as "Downloaded" in the library view
    MarkAsQueued(Vec<ArticleId>),
    /// Sets the given article as Not Downloaded in the library view
    MarkAsUnqueued(ArticleId),
}

#[derive(PartialEq, Properties)]
pub(crate) struct Props {
    pub queue_link: WeakComponentLink<Queue>,
    pub library_link: WeakComponentLink<Library>,
}

impl Component for Library {
    type Message = LibraryMsg;
    type Properties = Props;

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        let lib_link = ctx.link().clone();

        match msg {
            LibraryMsg::SetCatalog(catalog) => {
                self.err = None;
                self.catalog = Some(catalog);
            }
            LibraryMsg::SetError(err) => {
                self.catalog = None;
                self.err = Some(err);
            }
            LibraryMsg::FetchCatalog => {
                ctx.link().send_future(async move {
                    match fetch_catalog().await {
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
                        let article = match fetch_article(&id, &title, lib_link).await {
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
            LibraryMsg::SetDownloadProgress { id, progress } => {
                // Update the article's download progress
                self.download_progresses
                    .insert(id.clone(), DownloadProgress::InProgress(progress));
            }
            LibraryMsg::PassArticleToQueue(queue_entry) => {
                // Mark the article as downloaded
                self.download_progresses
                    .insert(queue_entry.id.clone(), DownloadProgress::Done);

                // If we saved an article, pass the notif to the queue
                ctx.props()
                    .queue_link
                    .borrow()
                    .clone()
                    .unwrap()
                    .send_message(QueueMsg::Add(queue_entry));
            }

            LibraryMsg::MarkAsQueued(ids) => {
                // Mark the article as downloaded
                ids.iter().for_each(|id| {
                    self.download_progresses
                        .insert(id.clone(), DownloadProgress::Done);
                })
            }

            LibraryMsg::MarkAsUnqueued(id) => {
                // Mark the article as not downloaded
                self.download_progresses.remove(&id);
            }
        }

        true
    }

    fn create(ctx: &Context<Self>) -> Self {
        // Set the queue link to this Library
        ctx.props()
            .library_link
            .borrow_mut()
            .replace(ctx.link().clone());

        // Kick of a future that will fetch the article list
        ctx.link().send_future(async move {
            match fetch_catalog().await {
                Ok(list) => LibraryMsg::SetCatalog(list),
                Err(e) => LibraryMsg::SetError(e.into()),
            }
        });

        // Save the pageshow callback and set it on document.window. This is so that when you hit
        // the back button from adding an article, it will try to reload the catalog.
        let lib_link = ctx.link().clone();
        let pageshow_cb = Closure::new(move |evt: PageTransitionEvent| {
            // If the page is loading from a cache, force it to reload the library
            if evt.persisted() {
                lib_link.send_message(LibraryMsg::FetchCatalog)
            }
        });
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
                .map(|metadata| {
                    let meta = metadata.clone();
                    let link = ctx.link().clone();
                    let download_progress = self
                        .download_progresses
                        .get(&ArticleId(meta.id.clone()))
                        .cloned();

                    render_lib_item(meta, link, download_progress)
                })
                .collect::<Html>();
            html! {
                <section title="Library">
                    <div id="libraryHeader">
                        <h2>{ "Library" }</h2>
                        <span id="addArticle">
                            <Link<Route> to={Route::Add}>
                                { "Add Article" }
                            </Link<Route>>
                        </span>
                    </div>
                    <table role="list" aria-label="Library catalog">
                        { rendered_list }
                    </table>
                <p
                    id="libErrors"
                    style={ "color: red;" }
                    aria-live="assertive"
                    aria-role="alert" title="errors">
                </p>
                </section>
            }
        } else {
            Default::default()
        }
    }
}
