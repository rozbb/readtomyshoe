use crate::tts::{get_api_key, tts, TtsRequest};
use common::ArticleSubmission;

use std::{fs::OpenOptions, io::Write, path::Path};

use anyhow::anyhow;
use axum::{extract::Extension, response::IntoResponse, routing::post, Json, Router};

struct AddArticleError(anyhow::Error);

impl From<anyhow::Error> for AddArticleError {
    fn from(error: anyhow::Error) -> Self {
        Self(error)
    }
}

impl axum::response::IntoResponse for AddArticleError {
    fn into_response(self) -> axum::response::Response {
        // Log the error and return it
        let err_str = self.0.to_string();
        tracing::error!("{}", err_str);
        (axum::http::StatusCode::INTERNAL_SERVER_ERROR, err_str).into_response()
    }
}

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
) -> Result<impl IntoResponse, AddArticleError> {
    // Open a new MP3 file. Fail if the file already exists
    let savepath = Path::new(&audio_blob_dir)
        .join(&title)
        .with_extension("mp3");
    let mut savefile = OpenOptions::new()
        .write(true)
        .create(false)
        .create_new(true)
        .open(savepath)
        .map_err(|e| anyhow!("Couldn't open savefile: {:?}", e))?;

    // TODO: Delete the savefile if an error occurs

    let api_key = get_api_key().map_err(|e| anyhow!("Failed to get Google API key: {:?}", e))?;

    // TODO: Internationalize this to use the correct stop character for the given language
    // Include the title at the top of the article.
    let text = format!("{title}. {body}");
    let req = TtsRequest {
        text,
        use_wavenet: false,
    };

    // Make the TTS request
    let bytes = tts(&api_key, req)
        .await
        .map_err(|e| anyhow!("TTS failed: {:?}", e))?;

    // Save the file
    savefile
        .write_all(&bytes)
        .map_err(|e| anyhow!("Save failed: {:?}", e))?;

    Ok(title)
}
