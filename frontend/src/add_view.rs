use common::ArticleSubmission;

use anyhow::{bail, Error as AnyError};
use gloo_net::http::Request;
use wasm_bindgen::JsValue;
use yew::prelude::*;

const TITLE_FORM_ID: &str = "article-title-input";
const BODY_FORM_ID: &str = "article-body-input";

/// Fetches the list of articles
async fn add_article(submission: &ArticleSubmission) -> Result<(), AnyError> {
    tracing::debug!("Adding article {:?}", submission);
    let resp = Request::post("/api/add-article")
        .json(&submission)?
        .send()
        .await
        .map_err(|e| AnyError::from(e).context("Error POSTing to /add-article"))?;

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

/// Retrives the value of the element with the given ID
fn get_elem_value(id: &str) -> String {
    let doc = gloo_utils::document();
    let elem = doc.get_element_by_id(id).unwrap();
    js_sys::Reflect::get(&elem, &JsValue::from_str("value"))
        .unwrap()
        .as_string()
        .unwrap()
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
        let add_callback = Callback::from(move |_| {
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
            let submission = ArticleSubmission { title, body };
            link.send_message(AddMsg::AddProgress("Converting to speech...".to_string()));

            tracing::debug!("Submitting {:?}", submission);

            // Make the submission
            link.send_future(async move {
                if let Err(e) = add_article(&submission).await {
                    // On error, send the error
                    AddMsg::SetError(e)
                } else {
                    // On success, say so
                    AddMsg::AddProgress("Success!".to_string())
                }
            });
        });

        let err_str = self
            .err
            .as_ref()
            .map(|e| format!("{:?}", e))
            .unwrap_or("".to_string());

        html! {
            <div id="add">
                <h2>{ "Add Article" }</h2>
                <section>
                    <p>
                        <label for={TITLE_FORM_ID}>{ "Article title" }</label>
                        <input type="text" id={TITLE_FORM_ID} required=true />
                    </p>
                    <p>
                        <label for={BODY_FORM_ID}>{ "Article body" }</label>
                        <textarea id={BODY_FORM_ID} rows="10" cols="33" required=true></textarea>
                    </p>
                </section>
                <section>
                    <button onclick={add_callback}>{ "Submit" }</button>
                </section>
                <section id="progress">
                    <p>
                        { self.progress.join(" ") }
                    </p>
                </section>
                <section id="errors">
                    <p style={ "color: red;" }>
                        { err_str }
                    </p>
                </section>

            </div>
        }
    }
}
