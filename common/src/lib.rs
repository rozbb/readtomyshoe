use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ArticleList {
    pub titles: Vec<String>,
}

impl ArticleList {
    pub fn new(titles: Vec<String>) -> Self {
        ArticleList { titles }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ArticleSubmission {
    pub title: String,
    pub body: String,
}
