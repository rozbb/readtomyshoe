use common::{ArticleTextSubmission, ArticleUrlSubmission, MAX_TITLE_UTF16_CODEUNITS};

use anyhow::{anyhow, bail, Error as AnyError};
use gloo_net::http::Request;
use wasm_bindgen::JsValue;
use yew::{html::Scope, prelude::*};

const URL_FORM_ID: &str = "article-url-input";
const TITLE_FORM_ID: &str = "article-title-input";
const BODY_FORM_ID: &str = "article-body-input";

/// POSTs the given ArticleTextSubmission to the server for conversion
async fn submit_article_text(submission: &ArticleTextSubmission) -> Result<(), AnyError> {
    tracing::debug!("Adding article {:?}", submission);
    let endpoint = "/api/add-article-by-text";
    let resp = Request::post(endpoint)
        .json(&submission)?
        .send()
        .await
        .map_err(|e| anyhow!("Error POSTing to {endpoint}: {}", e))?;

    if !resp.ok() {
        bail!(
            "Error adding article \"{}\" ({}; {:?})",
            submission.title,
            resp.status_text(),
            resp.text().await
        );
    }

    Ok(())
}

/// POSTs the given ArticleTextSubmission to the server for fetching and conversion
async fn submit_article_url(submission: &ArticleUrlSubmission) -> Result<(), AnyError> {
    tracing::debug!("Adding article {:?}", submission);
    let endpoint = "/api/add-article-by-url";
    let resp = Request::post(endpoint)
        .json(&submission)?
        .send()
        .await
        .map_err(|e| anyhow!("Error POSTing to {endpoint}: {}", e))?;

    if !resp.ok() {
        bail!(
            "Error adding article \"{}\". {}. {}",
            submission.url,
            resp.status_text(),
            resp.text().await.unwrap_or("".to_string())
        );
    }

    Ok(())
}

/// Retrives the value of the element with the given ID
fn get_elem_value(id: &str) -> String {
    let doc = gloo_utils::document();
    let elem = doc.get_element_by_id(id).unwrap();
    js_sys::Reflect::get(&elem, &JsValue::from_str("value"))
        .unwrap()
        .as_string()
        .unwrap()
}

/// POSTs the article title and body to the server for conversion
fn add_by_text_cb(link: Scope<Add>) {
    // Collect the title and body
    let title = get_elem_value(TITLE_FORM_ID);
    let body = get_elem_value(BODY_FORM_ID);

    if title.is_empty() || body.is_empty() {
        gloo_utils::window()
            .alert_with_message("Must fill out title and body")
            .unwrap();
        return;
    }

    // Construct the submission and update the progress
    let submission = ArticleTextSubmission { title, body };
    link.send_message(AddMsg::AddProgress("Converting to speech...".to_string()));

    tracing::debug!("Submitting {:?}", submission);

    // Make the submission
    link.send_future(async move {
        if let Err(e) = submit_article_text(&submission).await {
            // On error, send the error
            AddMsg::SetError(e)
        } else {
            // On success, say so
            AddMsg::AddProgress("Success!".to_string())
        }
    });
}

/// POSTs the article url to the server for fetching and conversion
fn add_by_url_cb(link: Scope<Add>) {
    // Collect the article URL
    let url = get_elem_value(URL_FORM_ID);

    if url.is_empty() {
        gloo_utils::window()
            .alert_with_message("Must fill out the URL")
            .unwrap();
        return;
    }

    // Construct the submission and update the progress
    let submission = ArticleUrlSubmission { url };
    link.send_message(AddMsg::AddProgress(
        "Fetching and converting article...".to_string(),
    ));

    tracing::debug!("Submitting {:?}", submission);

    // Make the submission
    link.send_future(async move {
        if let Err(e) = submit_article_url(&submission).await {
            // On error, send the error
            AddMsg::SetError(e)
        } else {
            // On success, say so
            AddMsg::AddProgress("Success!".to_string())
        }
    });
}

#[derive(Default)]
pub(crate) struct Add {
    err: Option<AnyError>,
    progress: Vec<String>,
}

pub enum AddMsg {
    SetError(AnyError),
    AddProgress(String),
}

impl Component for Add {
    type Message = AddMsg;
    type Properties = ();

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            AddMsg::SetError(e) => {
                self.err = Some(e);
            }
            AddMsg::AddProgress(p) => {
                self.progress.push(p);
            }
        }
        true
    }

    fn create(_ctx: &Context<Self>) -> Self {
        Add::default()
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link().clone();
        let link2 = ctx.link().clone();
        let add_text_callback = Callback::from(move |_| add_by_text_cb(link.clone()));
        let add_url_callback = Callback::from(move |_| add_by_url_cb(link2.clone()));

        let err_str = self
            .err
            .as_ref()
            .map(|e| format!("{}", e))
            .unwrap_or("".to_string());

        html! {
            <main>
                <h1>{ "Add article" }</h1>
                <p>{
                    "You may add an article either by providing a URL, or by pasting the title
                    and body text"
                }</p>
                <fieldset>
                    <legend><h2>{ "Add article by URL" }</h2></legend>
                    <div class="field">
                        <label for={URL_FORM_ID}>{ "Article URL:" }</label>
                        <input type="text" id={URL_FORM_ID} required=true />
                    </div>
                    <button type="submit" onclick={add_url_callback}>{ "Submit" }</button>
                </fieldset>
                <fieldset>
                    <legend><h2>{ "Add article by text" }</h2></legend>
                    <div class="field">
                        <label for={TITLE_FORM_ID}>{ "Article title:" }</label>
                        <input
                            type="text"
                            id={TITLE_FORM_ID}
                            maxlength={MAX_TITLE_UTF16_CODEUNITS.to_string()}
                            required=true
                        />
                    </div>
                    <div class="field">
                        <label for={BODY_FORM_ID}>{ "Article body:" }</label>
                        <textarea id={BODY_FORM_ID} rows="10" cols="33" required=true></textarea>
                    </div>
                    <div>
                        <button type="submit" onclick={add_text_callback}>{ "Submit" }</button>
                    </div>
                </fieldset>
                <section aria-live="polite" id="progress" title="progress">
                    <p>
                        { self.progress.join(" ") }
                    </p>
                </section>
                <section role="alert" id="errors" title="errors">
                    <p style={ "color: red;" }>
                        { err_str }
                    </p>
                </section>
            </main>
        }
    }
}
