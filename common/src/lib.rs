use serde::{Deserialize, Serialize};

/// Contains all the metadata about an article
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArticleMetadata {
    /// The title of the article
    pub title: String,
    /// The datetime the article was last modified, in seconds since Unix epoch
    pub unix_time_modified: Option<u64>,
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
