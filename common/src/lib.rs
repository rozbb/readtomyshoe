use serde::{Deserialize, Serialize};

/// The maximum allowed length of a title, in UTF-16 code units
pub const MAX_TITLE_UTF16_CODEUNITS: usize = 300;

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

impl ArticleTextSubmission {
    /// Converts this submission into its serialized string form
    pub fn serialize(&self) -> String {
        // TODO: Internationalize this to use the correct stop character for the given language
        // Include the title at the top of the article.
        format!("{}. {}", self.title, self.body)
    }
}

/// The request type for when the client sends just the article's URL
#[derive(Debug, Serialize, Deserialize)]
pub struct ArticleUrlSubmission {
    pub url: String,
}
