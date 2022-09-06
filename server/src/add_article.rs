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

#[derive(Debug)]
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
pub(crate) fn setup(router: Router, max_article_len: usize, audio_blob_dir: &str) -> Router {
    router.nest(
        "/api",
        Router::new()
            .route("/add-article-by-text", post(add_article_by_text_endpoint))
            .route("/add-article-by-url", post(add_article_by_url_endpoint))
            .layer(Extension((max_article_len, audio_blob_dir.to_string()))),
    )
}

/// Converts the given article contents to speech, and returns the new filename
async fn add_article_by_text_endpoint(
    Json(article): Json<ArticleTextSubmission>,
    Extension((max_article_len, audio_blob_dir)): Extension<(usize, String)>,
) -> Result<String, AddArticleError> {
    // Just call down to add_article_by_text
    tracing::debug!("Adding article by text: '{}'", article.title);
    let meta = match add_article_by_text(&article, max_article_len, &audio_blob_dir).await {
        Ok(m) => m,
        Err(e) => {
            tracing::error!("Error adding by text: {:?}", e);
            return Err(e);
        }
    };

    // Save the metadata in the ID3 tags
    let _ = save_metadata(&meta, &audio_blob_dir)
        .map_err(|e| tracing::error!("Error saving metadata: {e}"));

    Ok(meta.id)
}

/// Fetches the article at the given URL, converts it to speech, and returns the new filename
async fn add_article_by_url_endpoint(
    Json(ArticleUrlSubmission { url }): Json<ArticleUrlSubmission>,
    Extension((max_article_len, audio_blob_dir)): Extension<(usize, String)>,
) -> Result<String, AddArticleError> {
    tracing::debug!("Adding article by URL: {url}");
    let meta = match add_article_by_url(&url, max_article_len, &audio_blob_dir).await {
        Ok(m) => m,
        Err(e) => {
            tracing::error!("Error adding by url: {:?}", e);
            return Err(e);
        }
    };

    // Save the metadata in the ID3 tags
    let _ = save_metadata(&meta, &audio_blob_dir)
        .map_err(|e| tracing::error!("Error saving metadata: {e}"));

    Ok(meta.id)
}

/// The real logic. Converts the given article contents to speech, and returns the new filename
async fn add_article_by_text(
    article: &ArticleTextSubmission,
    max_article_len: usize,
    audio_blob_dir: &str,
) -> Result<ArticleMetadata, AddArticleError> {
    tracing::debug!("Processing article with title '{}'", article.title);

    if article.body.len() > max_article_len {
        Err(anyhow!(
            "Article too long. This server only permits articles of up to {} characters.",
            max_article_len
        ))?;
    }

    let id = derive_article_id(&article);

    // Fail if the article already exists
    let savepath = Path::new(&audio_blob_dir).join(&id).with_extension("mp3");
    if savepath.exists() {
        Err(anyhow!("File '{:?}' already exists", savepath))?;
    }

    // Open a temp file. This is so that list-articles won't try to read it while we're writing.
    // Again, fail if the file exists.
    let tmp_savepath = Path::new(&audio_blob_dir)
        .join(&id)
        .with_extension("mp3.tmp");
    let mut tmp_savefile = OpenOptions::new()
        .write(true)
        .create(false)
        .create_new(true)
        .open(&tmp_savepath)
        .map_err(|e| anyhow!("Couldn't open tmp savefile '{:?}': {:?}", tmp_savepath, e))?;

    // Try to do a TTS and save to the savefile. On error, make sure to clean up the empty file
    tts_to_file(&mut tmp_savefile, &article)
        .await
        .map_err(|e| {
            // Remove the file
            if let Err(f) = fs::remove_file(&tmp_savepath) {
                let context = format!("could not delete {id}: {f}");
                e.0.context(context).into()
            } else {
                e
            }
        })?;

    // TTS was successful, change the filename
    std::fs::rename(&tmp_savepath, &savepath)
        .map_err(|e| anyhow!("could not rename {:?} to {:?}: {e}", tmp_savepath, savepath))?;

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
    max_article_len: usize,
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
        Err(anyhow!("Text extraction failed"))?;
    }

    // Convert the CLI output from JSON and turn it into a `ArticleTextSubmission`
    let parsed_res: ExtractedArticle =
        serde_json::from_slice(&output.stdout).map_err(|_| anyhow!("Text extraction failed"))?;
    let text_submission = ArticleTextSubmission {
        title: parsed_res.title,
        body: parsed_res.text,
    };

    // Now that we have the article body, call down to add_article_by_text
    let mut meta = add_article_by_text(&text_submission, max_article_len, audio_blob_dir).await?;
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
