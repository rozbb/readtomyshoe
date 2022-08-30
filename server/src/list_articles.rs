use crate::util::get_metadata;

use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use common::{ArticleMetadata, LibraryCatalog};

use axum::{extract::Extension, http::StatusCode, routing::get, Json, Router};
use tower_http::compression::CompressionLayer;

/// The in-memory metadata cache of all the articles in the library. There is currently no way to
/// invalidate the cache, so if a file changes, the server needs to be restarted.
type LibraryCache = Arc<Mutex<BTreeMap<PathBuf, ArticleMetadata>>>;

// Sets the /api/list-articles route
pub(crate) fn setup(router: Router, audio_blob_dir: &str) -> Router {
    router.nest(
        "/api",
        Router::new()
            .route("/list-articles", get(list_articles))
            .layer(Extension(audio_blob_dir.to_string()))
            .layer(Extension(LibraryCache::default()))
            .layer(CompressionLayer::new()),
    )
}

/// Lists the articles in the audio blob directory
async fn list_articles(
    Extension(audio_blob_dir): Extension<String>,
    Extension(metadata_cache): Extension<LibraryCache>,
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
            let path = entry.path();

            // Don't list non-MP3 values
            if path.extension() != Some(OsStr::new("mp3")) {
                return None;
            }

            // Try to open the metadata cache
            if let Ok(mut cache) = metadata_cache.lock() {
                let mut already_cached = true;

                // See if the metadata is in the cache. If so, return it
                let meta = cache.get(&path).cloned().or_else(|| {
                    // If this file isn't in the cache, get the metadata
                    already_cached = false;
                    get_metadata(&entry)
                        .map_err(|e| tracing::error!("Could not extract metadata: {e}"))
                        .ok()
                });

                // If the file wasn't in the cache and metadata extraction succeeded, put the
                // metadata in the cache
                if !already_cached {
                    meta.as_ref().map(|m| cache.insert(path, m.clone()));
                }

                meta
            } else {
                // If the cache lock is poisoned, just get the metadata from the file
                get_metadata(&entry)
                    .map_err(|e| tracing::error!("Could not extract metadata: {e}"))
                    .ok()
            }
        })
        .collect::<Vec<ArticleMetadata>>();

    // Package the metadata and sort by time modified, most recently modified first
    let mut library_catalog = LibraryCatalog(metadatas);
    library_catalog
        .0
        .sort_by_key(|meta| std::cmp::Reverse(meta.datetime_added));

    // Done
    Ok(Json(library_catalog))
}
