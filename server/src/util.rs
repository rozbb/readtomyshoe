use common::ArticleTextSubmission;

use blake2::{Blake2s256, Digest};

/// Filenames in `audio_blobs` are of the form `TITLE-HASH.mp3`. This is maximum number of bytes
/// allowed in `TITLE`. This MUST be less than 256.
const FILENAME_TITLE_MAXLEN: usize = 20;

/// Filenames in `audio_blobs` are of the form `TITLE-HASH.mp3`. This is the size of the hash
/// BEFORE it is encoded in zbase32.
const ARTICLE_HASH_BITLEN: u64 = 128;

/// Truncates a string to occupy at most `n` bytes
fn truncate_to_bytes(s: &str, n: usize) -> &str {
    if n < 4 {
        panic!("Cannot guarantee a truncation with less than 4 bytes");
    }

    // Count the bytes of each character
    let mut byte_count = 0;
    for (i, c) in s.chars().enumerate() {
        byte_count += c.len_utf8();

        // If this character puts us over the byte limit, return the string up to and not including
        // this character
        if byte_count > n {
            return &s[..i];
        }
    }

    s
}

/// Computes the zbase32 encoded hash of the given article. The output length is ARTICLE_HASH_LEN.
/// Panics: if `title.len() > 255`.
fn hash_article(ArticleTextSubmission { title, body }: &ArticleTextSubmission) -> String {
    let mut h = Blake2s256::default();
    let title_len: u8 = title.len().try_into().unwrap();

    // Compute H(title_len || title || body)
    h.update([title_len]);
    h.update(title);
    h.update(body);

    // Now get the hash, truncate to ARTICLE_HASH_BITLEN, and convert to zbase32
    let digest = h.finalize();
    zbase32::encode(&digest, ARTICLE_HASH_BITLEN)
}

pub fn derive_article_id(article: &ArticleTextSubmission) -> String {
    let truncated_title = truncate_to_bytes(&article.title, FILENAME_TITLE_MAXLEN);
    let hash = hash_article(&article);
    format!("{truncated_title}-{hash}.mp3")
}
