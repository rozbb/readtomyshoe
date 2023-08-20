use crate::{error::RtmsError, util::get_metadata};

use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use common::{ArticleMetadata, LibraryCatalog};

use anyhow::bail;
use axum::{
    extract::Extension, headers::ContentType, response::IntoResponse, routing::get, Json, Router,
    TypedHeader,
};
use format_xml::{format as xformat, write as xwrite};
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
            .route("/feed", get(get_rss))
            .layer(Extension(audio_blob_dir.to_string()))
            .layer(Extension(LibraryCache::default()))
            .layer(CompressionLayer::new()),
    )
}

/// Lists the articles in the audio blob directory
async fn list_articles(
    Extension(audio_blob_dir): Extension<String>,
    Extension(metadata_cache): Extension<LibraryCache>,
) -> Result<Json<LibraryCatalog>, RtmsError> {
    // Get the catalog
    let mut library_catalog = get_library_catalog(audio_blob_dir, metadata_cache)?;
    // Sort by time modified, most recently modified first
    library_catalog
        .0
        .sort_by_key(|meta| std::cmp::Reverse(meta.datetime_added));

    // Done
    Ok(Json(library_catalog))
}

use crate::util::epoch_secs_to_datetime;

/// Builds an RSS feed from the existing library catalog
pub(crate) async fn get_rss(
    Extension(audio_blob_dir): Extension<String>,
    Extension(metadata_cache): Extension<LibraryCache>,
) -> Result<impl IntoResponse, RtmsError> {
    // Get the catalog
    let library_catalog = get_library_catalog(audio_blob_dir, metadata_cache)?;

    fn render_item(f: &mut std::fmt::Formatter, item: &ArticleMetadata) -> std::fmt::Result {
        // Convert the time added to an RFC 2822 string, or the empty string if it doesn't exist
        let datetime_added = item
            .datetime_added
            .map(epoch_secs_to_datetime)
            .as_ref()
            .map(chrono::DateTime::to_rfc2822)
            .unwrap_or(String::new());

        // The URL where the MP3 for this item lives
        let mp3_url = {
            let filename = format!("{}.mp3", item.id);
            let encoded_title = urlencoding::encode(&filename);
            format!("http://localhost:9382/api/audio-blobs/{encoded_title}")
        };

        // An <a> link for the source of this article, if one exists
        let source_text = item
            .source_url
            .as_ref()
            .map(|url| {
                xformat! {
                    <a href={url}>"Source"</a>
                }
            })
            .unwrap_or(String::new());

        // The duration of this article, rounded to the nearest second, if it exists
        let duration_text = item
            .duration
            .as_ref()
            .map(|dur| {
                let mut secs = dur.as_secs();
                if dur.subsec_millis() >= 500 {
                    secs += 1;
                }
                xformat! {
                    <itunes:duration>{secs}</itunes:duration>
                }
            })
            .unwrap_or(String::new());

        xwrite!(f,
            <item locked="false" ads="false" spons="false">
              <title>{item.title}</title>
              <pubDate>{datetime_added}</pubDate>
              |f| f.write_str(&duration_text)?;
              <enclosure url={mp3_url} length="0" type="audio/mpeg" />
              <itunes:explicit>"no"</itunes:explicit>
              <link />
              <itunes:episodeType>"full"</itunes:episodeType>
              <itunes:summary>{source_text}</itunes:summary>
              <description>{source_text}</description>
            </item>
        )
    }

    // We have to split this out because the expression gets too big otherwise
    fn render_channel_header(f: &mut std::fmt::Formatter) -> std::fmt::Result {
        xwrite!(f,
          <title>"ReadToMyShoe"</title>
          <language>"en"</language>
          <copyright>"Owners"</copyright>
          <itunes:author>"ReadToMyShoe"</itunes:author>
          <itunes:subtitle />
          <itunes:summary>"ReadToMyShoe's Feed"</itunes:summary>
          <description>"ReadToMyShoe's Feed"</description>
          <itunes:explicit>"yes"</itunes:explicit>
        )?; // Need to break this up because it reaches the recursion limit :(
        xwrite!(f,
            <itunes:owner>
              <itunes:name>"ReadToMyShoe"</itunes:name>
            </itunes:owner>
            <itunes:type>"episodic"</itunes:type>
            <itunes:image href="http://localhost:9382/rtms-color-512x512.png" />
            <image>
              <url>"rtms-color-512x512.png"</url>
              <link>"nourl"</link>
              <title>"ReadToMyShoe"</title>
            </image>
            <itunes:category text="News &amp; Politics" />
        )
    }

    let xml = xformat! {
        <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <?xml-stylesheet type="text/xsl" media="screen" href="/template/rss.xsl"?>
        <rss
            version="2.0"
            xmlns:atom="http://www.w3.org/2005/Atom"
            xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd"
            xmlns:media="http://search.yahoo.com/mrss/">
        <channel>
            // Render the channel beginning
            |f| render_channel_header(f)?;

            // Render the items in the channel
            for item in library_catalog.0.iter() {
                |f| render_item(f, item)?;
            }
        </channel>
        </rss>
    };

    Ok((TypedHeader(ContentType::xml()), xml.to_string()))
}

/// Fetches the library catalog, either reading from the cache, or reading from disk if the cache
/// is missing data
fn get_library_catalog(
    audio_blob_dir: String,
    metadata_cache: LibraryCache,
) -> Result<LibraryCatalog, anyhow::Error> {
    // Try to open the directory
    let dir: fs::ReadDir = match fs::read_dir(audio_blob_dir) {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("error reading dir {}", e);
            bail!("error reading dir {}", e)
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

    Ok(LibraryCatalog(metadatas))
}
