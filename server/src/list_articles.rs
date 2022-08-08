use crate::util::get_metadata;

use std::{
    ffi::OsStr,
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use common::{ArticleMetadata, LibraryCatalog};

use axum::{extract::Extension, http::StatusCode, routing::get, Json, Router};

// Sets the /api/list-articles route
pub(crate) fn setup(router: Router, audio_blob_dir: &str) -> Router {
    router.nest(
        "/api",
        Router::new()
            .route("/list-articles", get(list_articles))
            .layer(Extension(audio_blob_dir.to_string())),
    )
}

/// Lists the articles in the audio blob directory
async fn list_articles(
    Extension(audio_blob_dir): Extension<String>,
) -> Result<Json<LibraryCatalog>, StatusCode> {
    // Try to open the directory
    let dir: fs::ReadDir = match fs::read_dir(audio_blob_dir) {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("error reading dir {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // List the directory and collect the metadata
    let metadatas = dir
        .filter_map(|entry| {
            // Check for a listing error
            if let Err(e) = entry {
                tracing::error!("Could not list file in audio_blobs/: {:?}", e);
                return None;
            }
            let entry = entry.unwrap();

            // Don't list non-MP3 values
            if entry.path().extension() != Some(OsStr::new("mp3")) {
                return None;
            }

            // As a backup for the added time, get the time the file was last modified
            let time_modified: Option<SystemTime> =
                entry.metadata().and_then(|m| m.modified()).ok();
            // Convert the time to seconds since epoch
            let unix_time_modified = time_modified
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|dur| dur.as_secs());

            // Get the metadata
            match get_metadata(entry.path(), unix_time_modified) {
                Ok(meta) => Some(meta),
                Err(e) => {
                    tracing::error!("Could not extract metadata: {e}");
                    None
                }
            }

            /*

            // Get the "file stem", i.e., the filename without hte extension
            let title = entry
                .path()
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_string)
                .unwrap_or("[could not get filename]".to_string());

            // Now get the time the file was last modified
            let time_modified: Option<SystemTime> =
                entry.metadata().and_then(|m| m.modified()).ok();
            // Convert the time to seconds since epoch
            let unix_time_modified = time_modified
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|dur| dur.as_secs());

            // Return the metadata
            Some(ArticleMetadata {
                id: title.clone(),
                title,
                datetime_added: unix_time_modified,
                source_url: None,
            })
                */
        })
        .collect::<Vec<ArticleMetadata>>();
    let mut library_catalog = LibraryCatalog(metadatas);

    // Now sort by time modified, most recently modified first
    library_catalog
        .0
        .sort_by_key(|meta| std::cmp::Reverse(meta.datetime_added));

    // Done
    Ok(Json(library_catalog))
}
