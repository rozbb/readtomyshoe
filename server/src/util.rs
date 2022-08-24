use common::{ArticleMetadata, ArticleTextSubmission};

use std::{
    fs::DirEntry,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{bail, Error as AnyError};
use blake2::{Blake2s256, Digest};
use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Utc};
use id3::{Tag, TagLike, Version};

/// Filenames in `audio_blobs` are of the form `TITLE-HASH.mp3`. This is maximum number of bytes
/// allowed in `TITLE`. This MUST be less than 256.
const FILENAME_TITLE_MAXLEN: usize = 20;

/// Filenames in `audio_blobs` are of the form `TITLE-HASH.mp3`. This is the size of the hash
/// BEFORE it is encoded in zbase32.
const ARTICLE_HASH_BITLEN: u64 = 128;

/// Truncates a string to occupy at most `n` bytes
fn truncate_to_bytes(s: &str, n: usize) -> &str {
    // A codepoint in UTF-8 is at most 4 bytes. So the smallest we can guarantee truncation to is 4
    // bytes.
    if n < 4 {
        panic!("Cannot guarantee a truncation with less than 4 bytes");
    }

    // Count the bytes of each character
    let mut byte_count = 0;
    for c in s.chars() {
        let char_size = c.len_utf8();

        // If this character puts us over the byte limit, return the string up to and not including
        // this character
        if byte_count + char_size > n {
            return &s[..byte_count];
        } else {
            // If this character is within the limit, add it to the count
            byte_count += char_size;
        }
    }

    s
}

/// Computes the zbase32 encoded hash of the given article. The output length is ARTICLE_HASH_LEN.
///
/// Panics: if `title.len() > 255`.
fn hash_article(ArticleTextSubmission { title, body }: &ArticleTextSubmission) -> String {
    let mut h = Blake2s256::default();
    // This should never be an issue because derive_article_id truncates the title to
    // FILENAME_TITLE_MAXLEN bytes, which is far less than 256
    let title_len: u8 = title.len().try_into().unwrap();

    // Compute H(title_len || title || body)
    h.update([title_len]);
    h.update(title);
    h.update(body);

    // Now get the hash, truncate to ARTICLE_HASH_BITLEN, and convert to zbase32
    let digest = h.finalize();
    zbase32::encode(&digest, ARTICLE_HASH_BITLEN)
}

/// Derives the unique ID of this article. It's of the form SHORTTITLE-HASH.mp3
pub fn derive_article_id(article: &ArticleTextSubmission) -> String {
    let truncated_title = truncate_to_bytes(&article.title, FILENAME_TITLE_MAXLEN);
    let hash = hash_article(&article);
    format!("{truncated_title}-{hash}.mp3")
}

/// Saves article metadata as ID3 tags in the MP3 file:
///
///     url -> Artist
///     title -> Title
///     date fetched  -> Recording Time
pub fn save_metadata(meta: &ArticleMetadata, audio_blob_dir: &str) -> Result<(), AnyError> {
    let savepath = Path::new(&audio_blob_dir)
        .join(&meta.id)
        .with_extension("mp3");

    // Set the ID3 title
    let mut tag = Tag::new();
    tag.set_title(&meta.title);

    // Set the ID3 recording date to be the date the article was added
    if let Some(added) = meta.datetime_added {
        let date = NaiveDateTime::from_timestamp(
            added
                .try_into()
                .expect("it is 2038 and chrono still uses i64 for unix time"),
            0,
        );
        let date = DateTime::<Utc>::from_utc(date, Utc);
        tag.set_date_recorded(id3::Timestamp {
            year: date.year(),
            month: Some(date.month() as u8),
            day: Some(date.day() as u8),
            hour: Some(date.hour() as u8),
            minute: Some(date.minute() as u8),
            second: Some(date.second() as u8),
        });
    }

    // Set the URL as the artist
    if let Some(url) = &meta.source_url {
        tag.set_artist(url);
    }

    // Now write
    tag.write_to_path(savepath, Version::Id3v24)
        .map_err(Into::into)
}

/// Gets article metadata from ID3 tags in the MP3 file:
///
///     url <- Artist
///     title <- Title
///     date fetched  <- Recording Time (or else Unix last modified time)
pub fn get_metadata(entry: &DirEntry) -> Result<ArticleMetadata, AnyError> {
    let path = entry.path();

    // The `last_modified_timestamp` is a backup in case the Recording Time isn't set
    let last_modified_timestamp: Option<u64> = {
        let time_modified: Option<SystemTime> = entry.metadata().and_then(|m| m.modified()).ok();
        // Convert the time to seconds since epoch
        time_modified
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|dur| dur.as_secs())
    };

    // The article ID is its filename. It better be unicode
    let id = match path.file_stem().unwrap().to_str() {
        Some(s) => s.to_string(),
        None => bail!("filename is not valid unicode"),
    };

    // Pick default metadata in case no ID3 tag exists
    let mut meta = ArticleMetadata {
        title: id.clone(),
        id,
        source_url: None,
        datetime_added: last_modified_timestamp,
    };

    // Try to get the metadata from the ID3 tags
    if let Ok(tag) = Tag::read_from_path(path) {
        // Try to get the ID3 title and source URL (URL is in the Artist field)
        meta.title = tag.title().unwrap_or(&meta.title).to_string();
        meta.source_url = tag.artist().map(str::to_string);

        // Extract the time recorded and convert it back to a unix timestamp. It's a pain
        let datetime_added = tag.date_recorded().and_then(|recorded| {
            let date = NaiveDate::from_ymd(
                recorded.year,
                recorded.month.unwrap_or(0) as u32,
                recorded.day.unwrap_or(0) as u32,
            );
            let time = NaiveTime::from_hms(
                recorded.hour.unwrap_or(0) as u32,
                recorded.minute.unwrap_or(0) as u32,
                recorded.second.unwrap_or(0) as u32,
            );
            let naive_datetime = NaiveDateTime::new(date, time);
            let datetime = DateTime::<Utc>::from_utc(naive_datetime, Utc);
            datetime.timestamp().try_into().ok()
        });
        meta.datetime_added = datetime_added.or(meta.datetime_added);
    }

    Ok(meta)
}

#[test]
fn test_title_truncation() {
    let title = "Money Stuff: AMC’s APEs Might Stick Around";
    assert_eq!(
        truncate_to_bytes(title, FILENAME_TITLE_MAXLEN),
        "Money Stuff: AMC’s"
    );
}
