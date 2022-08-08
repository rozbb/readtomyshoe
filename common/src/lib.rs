use serde::{Deserialize, Serialize};

/// Contains all the metadata about an article
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ArticleMetadata {
    /// The ID of the article
    pub id: String,
    /// The title of the article
    pub title: String,
    /// The datetime the article was added to the library
    pub datetime_added: Option<u64>,
    /// The URL this article was sourced from, if any
    pub source_url: Option<String>,
}

/// A library catalog is a list of article metadata
#[derive(Debug, Serialize, Deserialize)]
pub struct LibraryCatalog(pub Vec<ArticleMetadata>);

/// The request type for when the client sends the raw text of the article they want converted
#[derive(Debug, Serialize, Deserialize)]
pub struct ArticleTextSubmission {
    pub title: String,
    pub body: String,
}

/// The request type for when the client sends just the article's URL
#[derive(Debug, Serialize, Deserialize)]
pub struct ArticleUrlSubmission {
    pub url: String,
}
