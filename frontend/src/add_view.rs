use crate::{
    caching,
    queue_view::{CachedArticle, CachedArticleHandle, Queue, QueueMsg},
    WeakComponentLink,
};
use common::ArticleSubmission;

use anyhow::{bail, Error as AnyError};
use gloo_net::http::Request;
use wasm_bindgen::JsValue;
use yew::{html::Scope, prelude::*};

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
            "Error fetching article list {} ({})",
            resp.status(),
            resp.status_text()
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
}

pub enum AddMsg {
    SetError(Option<AnyError>),
}

impl Component for Add {
    type Message = AddMsg;
    type Properties = ();

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            AddMsg::SetError(e) => {
                self.err = e;
                true
            }
        }
    }

    fn create(_ctx: &Context<Self>) -> Self {
        Add::default()
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let add_callback = ctx.link().callback_future(|_: MouseEvent| async move {
            // Collect the title and body
            let title = get_elem_value(TITLE_FORM_ID);
            let body = get_elem_value(BODY_FORM_ID);

            if title.is_empty() || body.is_empty() {
                gloo_utils::window()
                    .alert_with_message("Must fill out title and body")
                    .unwrap();
                return AddMsg::SetError(None);
            }

            let submission = ArticleSubmission { title, body };

            tracing::debug!("Submitting {:?}", submission);

            // Make the submission
            let res = add_article(&submission).await.err();
            AddMsg::SetError(res)
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
                <section id="errors">
                    <p style={ "color: red;" }>
                        { err_str }
                    </p>
                </section>

            </div>
        }
    }
}
