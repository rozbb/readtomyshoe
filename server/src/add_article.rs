use crate::{
    error::RtmsError,
    lang::pick_tts_voice,
    tts::{get_api_key, tts, TtsRequest, VoiceQuality, VoiceType},
    util::{derive_article_id, get_mp3_duration, save_metadata, truncate_to_bytes, StrEncoding},
};
use common::{
    ArticleMetadata, ArticleTextSubmission, ArticleUrlSubmission, MAX_TITLE_UTF16_CODEUNITS,
};

use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    num::{NonZeroU32, NonZeroUsize},
    path::Path,
    sync::Arc,
    time::SystemTime,
};

use anyhow::{anyhow, Context};
use async_process::Command;
use axum::{extract::Extension, routing::post, Json, Router};
use governor::{
    clock::DefaultClock, middleware::NoOpMiddleware, state::direct::NotKeyed, state::InMemoryState,
    Quota, RateLimiter as BaseRateLimiter,
};
use serde::Deserialize;

type DefaultRateLimiter = BaseRateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>;

/// The rate limiter for TTS calls. The quota contains the quota for characters per minute.
#[derive(Clone)]
struct RateLimiter {
    base_rl: Arc<DefaultRateLimiter>,
    quota: Quota,
}

/// A portion of trafilatura's extracted text. The rest of the fields are: title, author, hostname,
/// date, categories, tags, fingerprint, id, license, comments, raw_text, source, source_hostname,
/// excerpt, text
#[derive(Deserialize)]
struct ExtractedArticle {
    title: String,
    text: String,
}

// Sets the /api/add-article route
pub(crate) fn setup(router: Router, max_chars_per_min: NonZeroU32, audio_blob_dir: &str) -> Router {
    // Set up the rate limiter for our TTS queries
    let quota = Quota::per_minute(max_chars_per_min);
    let tts_rate_limiter = RateLimiter {
        base_rl: Arc::new(DefaultRateLimiter::direct(quota.clone())),
        quota,
    };

    // Set up the routes
    router.nest(
        "/api",
        Router::new()
            .route("/add-article-by-text", post(add_article_by_text_endpoint))
            .route("/add-article-by-url", post(add_article_by_url_endpoint))
            .layer(Extension(tts_rate_limiter))
            .layer(Extension(audio_blob_dir.to_string())),
    )
}

/// Converts the given article contents to speech, and returns the new filename
async fn add_article_by_text_endpoint(
    Json(article): Json<ArticleTextSubmission>,
    Extension(tts_rate_limiter): Extension<RateLimiter>,
    Extension(audio_blob_dir): Extension<String>,
) -> Result<String, RtmsError> {
    // Just call down to add_article_by_text
    tracing::debug!("Adding article by text: '{}'", article.title);
    let meta = match add_article_by_text(&article, tts_rate_limiter, &audio_blob_dir).await {
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
    Extension(tts_rate_limiter): Extension<RateLimiter>,
    Extension(audio_blob_dir): Extension<String>,
) -> Result<String, RtmsError> {
    tracing::debug!("Adding article by URL: {url}");
    let meta = match add_article_by_url(&url, tts_rate_limiter, &audio_blob_dir).await {
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
    tts_rate_limiter: RateLimiter,
    audio_blob_dir: &str,
) -> Result<ArticleMetadata, RtmsError> {
    tracing::debug!("Processing article with title '{}'", article.title);

    // Serialize the article and get its bytelen
    let text = article.serialize();
    let text_len: NonZeroU32 = {
        let n = text.len();
        let nzn = NonZeroUsize::new(n).or(NonZeroUsize::new(1)).unwrap();
        NonZeroU32::try_from(nzn).with_context(|| "Article is is far too large")?
    };

    // If the article bytelen exceeds the limit, error out
    if tts_rate_limiter.base_rl.check_n(text_len).is_err() {
        Err(anyhow!(
            "Usage limit exceeded. This server processes at most {} letters per minute.",
            tts_rate_limiter.quota.burst_size().get(),
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
    tts_to_file(&mut tmp_savefile, text).await.map_err(|e| {
        // Remove the file
        if let Err(f) = fs::remove_file(&tmp_savepath) {
            let context = format!("could not delete {id}: {f}");
            e.context(context).into()
        } else {
            e
        }
    })?;

    // TTS was successful, change the filename
    std::fs::rename(&tmp_savepath, &savepath)
        .map_err(|e| anyhow!("could not rename {:?} to {:?}: {e}", tmp_savepath, savepath))?;

    // Measure its duration. This goes in metadata
    let article_duration = get_mp3_duration(&savepath).ok();

    // Get the current time. This is the official time the article was added to the library
    let unix_epoch_now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // The displayed titles can't be too long, since they'll destroy the user interface. If this
    // one is too long, truncate it to a reasonable size. The size is in UTF-16 code units because
    // that's what HTML <input maxlength=""> attributes are.
    let truncated_title = truncate_to_bytes(
        &article.title,
        2 * MAX_TITLE_UTF16_CODEUNITS,
        StrEncoding::Utf16,
    )
    .to_string();

    // Return the metadata
    Ok(ArticleMetadata {
        id,
        title: truncated_title,
        duration: article_duration,
        datetime_added: Some(unix_epoch_now),
        source_url: None,
    })
}

/// The real logic. Fetches the article at the given URL, converts it to speech, and returns the
/// new filename
async fn add_article_by_url(
    url: &str,
    tts_rate_limiter: RateLimiter,
    audio_blob_dir: &str,
) -> Result<ArticleMetadata, RtmsError> {
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
    let mut meta = add_article_by_text(&text_submission, tts_rate_limiter, audio_blob_dir).await?;
    // Add the URL to the metadata
    meta.source_url = Some(url.to_string());

    Ok(meta)
}

/// Converts an article to speech and saves to the given file
async fn tts_to_file(file: &mut File, text: String) -> Result<(), RtmsError> {
    let api_key = get_api_key().map_err(|e| anyhow!("Failed to get Google API key: {:?}", e))?;

    // Use the language detector to pick the TTS voice
    let voice_name = pick_tts_voice(&text, VoiceQuality::High, VoiceType::HighPitch)?;

    // Make the TTS request
    let req = TtsRequest { text, voice_name };
    let bytes = tts(&api_key, req)
        .await
        .map_err(|e| anyhow!("TTS failed: {:?}", e))?;

    // Save the file
    file.write_all(&bytes)
        .map_err(|e| anyhow!("Save failed: {:?}", e))?;

    Ok(())
}
