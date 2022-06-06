use std::fs;

use common::ArticleList;

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
) -> Result<Json<ArticleList>, StatusCode> {
    // Try to open the directory
    let dir: fs::ReadDir = match fs::read_dir(audio_blob_dir) {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("error reading dir {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // List the directory
    let article_list = dir
        .map(|d| d.map(|r| r.file_name().into_string().unwrap()))
        .collect::<Result<Vec<String>, std::io::Error>>()
        .map(ArticleList::new);

    if let Ok(l) = article_list {
        Ok(Json(l))
    } else {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
}
