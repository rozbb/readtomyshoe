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
use js_sys::{ArrayBuffer, Uint8Array};
use url::Url;
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use wasm_bindgen_futures::spawn_local;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    PageTransitionEvent, ReadableStream, ReadableStreamDefaultController,
    ReadableStreamDefaultReader,
};
use yew::{html::Scope, prelude::*};
use yew_router::prelude::*;

/// A helper function for methods that return JsValue as an error type
fn wrap_jserror(context_str: &'static str, v: JsValue) -> AnyError {
    anyhow::anyhow!("{context_str}: {:?}", v)
}

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

// This pattern was taken from
// https://github.com/AnthumChris/fetch-progress-indicators/blob/efaaaf073bc6927a803e5963a92ba9b11a585cc0/fetch-basic/supported-browser.js
async fn read_inc(
    num_bytes_expected: usize,
    reader: ReadableStreamDefaultReader,
    controller: ReadableStreamDefaultController,
) {
    let mut num_bytes_read = 0;

    let mut acc = String::new();

    loop {
        let chunk_promise = reader.read();
        let chunk = match JsFuture::from(chunk_promise).await {
            Ok(c) => c,
            Err(e) => {
                ReadableStreamDefaultController::error_with_e(&controller, &e);
                return;
            }
        };

        let done =
            js_sys::Reflect::get(&chunk, &JsValue::from_str("done")).unwrap() == JsValue::TRUE;

        if done {
            let _ = ReadableStreamDefaultController::close(&controller);
            return;
        } else {
            // Get the chunk and append it to the queue
            let val = js_sys::Reflect::get(&chunk, &JsValue::from_str("value")).unwrap();
            controller.enqueue_with_chunk(&val);

            // Update the number of bytes read
            let chunk_size = val.unchecked_into::<ArrayBuffer>().byte_length();
            num_bytes_read += chunk_size;

            let progress = 100.0 * num_bytes_read as f64 / num_bytes_expected as f64;
            tracing::error!("Fetched {progress}%");

            //let typed_buff: Uint8Array = Uint8Array::new(&val);
            //let slice_begin = audio_blob.len();
            //audio_blob.extend(core::iter::repeat(0u8).take(typed_buff.length() as usize));
            //typed_buff.copy_to(&mut audio_blob[slice_begin..]);

            //library_link.send_message(LibraryMsg::SetDownloadProgress {
            //    id: ArticleId(id.to_string()),
            //    progress: audio_blob.len() as f64 / blob_size as f64,
            //});
        }
    }
}

/// Fetches a specific article
async fn fetch_article(id: &str, title: &str) -> Result<CachedArticle, AnyError> {
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

    let blob_size = resp
        .headers()
        .get("content-length")
        .unwrap()
        .parse::<usize>()
        .unwrap();

    let reader = resp
        .body()
        .ok_or(AnyError::msg("could not get repsonse body"))?
        .get_reader()
        .dyn_into::<JsValue>()
        .unwrap();
    tracing::error!("reader is {:?}", reader);
    tracing::error!("reader type is {:?}", reader.js_typeof().as_string());
    let keys = js_sys::Reflect::own_keys(&reader).unwrap();
    for i in 0..keys.length() {
        let k = keys.get(i as u32);
        tracing::error!("reader has a key '{}'", k.as_string().unwrap());
    }

    let reader: ReadableStreamDefaultReader = reader.unchecked_into();
    /*
    let reader: ReadableStreamDefaultReader = reader
        .dyn_into()
        .map_err(|e| tracing::error!("Not a ReadableStreamDefault reader. {:?}", e))
        .unwrap();
    */

    // let pageshow_cb = Closure::new(move |evt| pageshow_callback(evt, link.clone()));
    let start_cb = Closure::once(move |controller: ReadableStreamDefaultController| {
        spawn_local(async move {
            read_inc(blob_size, reader, controller).await;
        })
    });

    let readable_stream_callbacks = js_sys::Object::default();
    js_sys::Reflect::set(
        readable_stream_callbacks.as_ref(),
        &JsValue::from_str("start"),
        start_cb.as_ref(),
    );

    let readable_stream =
        ReadableStream::new_with_underlying_source(&readable_stream_callbacks).unwrap();
    let resp = {
        let raw = web_sys::Response::new_with_opt_readable_stream(Some(&readable_stream)).unwrap();
        gloo_net::http::Response::from_raw(raw)
    };

    if !resp.ok() {
        bail!(
            "Error fetching article {} ({})",
            resp.status(),
            resp.status_text()
        );
    }

    tracing::info!("Got audio blob {:?}", resp);

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
fn render_lib_item(
    metadata: ArticleMetadata,
    library_link: Scope<Library>,
    download_progress: Option<DownloadProgress>,
) -> Html {
    let title = metadata.title.clone();
    let id = metadata.id.clone();
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
            DownloadProgress::InProgress => {
                html! {
                    <p id={elem_id} class="downloadProgress">{ "..." }</p>
                }
            }
            DownloadProgress::Done => {
                html! {
                    <p id={elem_id} class="downloadProgress">{ "✔︎" }</p>
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
                <span class="articleMetadata" title="Date added">{ date_added_str }</span>
                <span class="articleMetadata">{ url }</span>
            </td>
        </tr>
    }
}

/// Denotes whether a library item is downloading or done downloading
#[derive(Copy, Clone, Debug)]
enum DownloadProgress {
    InProgress,
    Done,
}

#[derive(Default)]
pub(crate) struct Library {
    err: Option<AnyError>,
    catalog: Option<LibraryCatalog>,
    download_progresses: BTreeMap<ArticleId, DownloadProgress>,
    _pageshow_action: Option<Closure<dyn 'static + Fn(PageTransitionEvent)>>,
}

pub(crate) enum LibraryMsg {
    SetCatalog(LibraryCatalog),
    SetError(AnyError),
    FetchArticle { id: String, title: String },
    FetchLibrary,
    PassArticleToQueue(QueueEntry),
    MarkAsQueued(Vec<ArticleId>),
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
                // Mark the fetch as in-progress
                self.download_progresses
                    .insert(ArticleId(id.clone()), DownloadProgress::InProgress);

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
