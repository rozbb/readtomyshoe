use crate::{
    tts::{get_api_key, tts, TtsRequest},
    util::{derive_article_id, save_metadata},
};
use common::{ArticleMetadata, ArticleTextSubmission, ArticleUrlSubmission};

use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::Path,
    time::SystemTime,
};

use anyhow::anyhow;
use async_process::Command;
use axum::{extract::Extension, routing::post, Json, Router};
use serde::Deserialize;

/// A portion of trafilatura's extracted text. The rest of the fields are: title, author, hostname,
/// date, categories, tags, fingerprint, id, license, comments, raw_text, source, source_hostname,
/// excerpt, text
#[derive(Deserialize)]
struct ExtractedArticle {
    title: String,
    text: String,
}

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
            .route("/add-article-by-text", post(add_article_by_text_endpoint))
            .route("/add-article-by-url", post(add_article_by_url_endpoint))
            .layer(Extension(audio_blob_dir.to_string())),
    )
}

/// Converts the given article contents to speech, and returns the new filename
async fn add_article_by_text_endpoint(
    Json(article): Json<ArticleTextSubmission>,
    Extension(audio_blob_dir): Extension<String>,
) -> Result<String, AddArticleError> {
    // Just call down to add_article_by_text
    tracing::debug!("Adding article by text: '{}'", article.title);
    let meta = add_article_by_text(&article, &audio_blob_dir).await?;

    // Save the metadata in the ID3 tags
    let _ = save_metadata(&meta, &audio_blob_dir)
        .map_err(|e| tracing::error!("Error saving metadata: {e}"));

    Ok(meta.id)
}

/// Fetches the article at the given URL, converts it to speech, and returns the new filename
async fn add_article_by_url_endpoint(
    Json(ArticleUrlSubmission { url }): Json<ArticleUrlSubmission>,
    Extension(audio_blob_dir): Extension<String>,
) -> Result<String, AddArticleError> {
    tracing::debug!("Adding article by URL: {url}");
    let meta = add_article_by_url(&url, &audio_blob_dir).await?;

    // Save the metadata in the ID3 tags
    let _ = save_metadata(&meta, &audio_blob_dir)
        .map_err(|e| tracing::error!("Error saving metadata: {e}"));

    Ok(meta.id)
}

/// The real logic. Converts the given article contents to speech, and returns the new filename
async fn add_article_by_text(
    article: &ArticleTextSubmission,
    audio_blob_dir: &str,
) -> Result<ArticleMetadata, AddArticleError> {
    tracing::debug!("Processing article with title '{}'", article.title);
    let id = derive_article_id(&article);

    // Open a new MP3 file. Fail if the file already exists
    let savepath = Path::new(&audio_blob_dir).join(&id).with_extension("mp3");
    let mut savefile = OpenOptions::new()
        .write(true)
        .create(false)
        .create_new(true)
        .open(&savepath)
        .map_err(|e| anyhow!("Couldn't open savefile '{:?}': {:?}", savepath, e))?;

    // Try to do a TTS and save to the savefile. On error, make sure to clean up the empty file
    match tts_to_file(&mut savefile, &article).await {
        Ok(_) => (),
        Err(e) => {
            // Remove the file
            let _ = fs::remove_file(savepath)
                .map_err(|e| tracing::error!("could not delete {id}: {e}"));
            // Return with the error
            return Err(e);
        }
    }

    // Get the current time. This is the official time the article was added to the library
    let unix_epoch_now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Return the metadata
    Ok(ArticleMetadata {
        id,
        title: article.title.clone(),
        datetime_added: Some(unix_epoch_now),
        source_url: None,
    })
}

/// The real logic. Fetches the article at the given URL, converts it to speech, and returns the
/// new filename
async fn add_article_by_url(
    url: &str,
    audio_blob_dir: &str,
) -> Result<ArticleMetadata, AddArticleError> {
    // TODO: Check earlier that trafilatura is present

    // Run trafilatura on the given URL
    let output = Command::new("../python_deps/bin/trafilatura")
        .env("PYTHONPATH", "../python_deps")
        .arg("--json")
        .arg("--URL")
        .arg(&url)
        .output()
        .await
        .map_err(|e| anyhow!("IO error running trafulatura: {:?}", e))?;
    // See if the command failed
    if !output.status.success() {
        Err(anyhow!(
            "Error running trafilatura: {}",
            String::from_utf8_lossy(&output.stderr)
        ))?;
    }

    // Convert the CLI output from JSON and turn it into a `ArticleTextSubmission`
    let parsed_res: ExtractedArticle = serde_json::from_slice(&output.stdout)
        .map_err(|e| anyhow!("Error parsing trafilatura JSON: {:?}", e))?;
    let text_submission = ArticleTextSubmission {
        title: parsed_res.title,
        body: parsed_res.text,
    };

    // Now that we have the article body, call down to add_article_by_text
    let mut meta = add_article_by_text(&text_submission, audio_blob_dir).await?;
    // Add the URL to the metadata
    meta.source_url = Some(url.to_string());

    Ok(meta)
}

/// Converts an article to speech and saves to the given file
async fn tts_to_file(
    file: &mut File,
    ArticleTextSubmission { title, body }: &ArticleTextSubmission,
) -> Result<(), AddArticleError> {
    let api_key = get_api_key().map_err(|e| anyhow!("Failed to get Google API key: {:?}", e))?;

    // TODO: Internationalize this to use the correct stop character for the given language
    // Include the title at the top of the article.
    let text = format!("{title}. {body}");
    let req = TtsRequest {
        text,
        use_wavenet: true,
    };

    // Make the TTS request
    let bytes = tts(&api_key, req)
        .await
        .map_err(|e| anyhow!("TTS failed: {:?}", e))?;

    // Save the file
    file.write_all(&bytes)
        .map_err(|e| anyhow!("Save failed: {:?}", e))?;

    Ok(())
}
