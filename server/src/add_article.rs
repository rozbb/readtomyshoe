use crate::tts::{get_api_key, tts, TtsRequest};
use common::ArticleSubmission;

use std::{
    ffi::OsString,
    fs::{File, OpenOptions},
    io::Write,
    path::Path,
};

use anyhow::Error;
use axum::{
    body::StreamBody, extract::Extension, http::StatusCode, response::IntoResponse, routing::post,
    Json, Router,
};
use tokio_stream::wrappers::ReceiverStream;

// Sets the /api/add-article route
pub(crate) fn setup(router: Router, audio_blob_dir: &str) -> Router {
    router.nest(
        "/api",
        Router::new()
            .route("/add-article", post(add_article))
            .layer(Extension(audio_blob_dir.to_string())),
    )
}

/// Lists the articles in the audio blob directory
async fn add_article(
    Json(ArticleSubmission { title, body }): Json<ArticleSubmission>,
    Extension(audio_blob_dir): Extension<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Open a new MP3 file. Fail if the file already exists
    let savepath = Path::new(&audio_blob_dir)
        .join(&title)
        .with_extension("mp3");
    let mut savefile = match OpenOptions::new()
        .write(true)
        .create(false)
        .create_new(true)
        .open(savepath)
    {
        Ok(f) => f,
        Err(e) => {
            let err_str = format!("Couldn't open savefile: {:?}", e);
            tracing::error!("{}", err_str);
            return Err((StatusCode::INTERNAL_SERVER_ERROR, err_str));
        }
    };

    let (resp_tx, resp_rx) = tokio::sync::mpsc::channel(10);

    tokio::spawn(async move {
        let api_key = match get_api_key() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("Failed to get Google API key: {:?}", e);
                return;
            }
        };

        // TODO: Internationalize this to use the correct stop character for the given language
        // Include the title at the top of the article.
        let req = TtsRequest {
            text: format!("{title}. {body}"),
            wavenet: false,
        };

        // Make the TTS request
        let bytes = match tts(&api_key, &req).await {
            Ok(b) => b,
            Err(e) => {
                tracing::error!("TTS failed: {:?}", e);
                return;
            }
        };

        // Save the file
        if let Err(e) = savefile.write_all(&bytes) {
            tracing::error!("Save failed: {:?}", e);
            return;
        }

        let r: Result<String, String> = Ok(title);
        if let Err(e) = resp_tx.send(r).await {
            tracing::error!("Failed to send body: {:?}", e);
            return;
        }
    });

    Ok(StreamBody::new(ReceiverStream::from(resp_rx)))
}
