use common::{ArticleMetadata, ArticleTextSubmission};

use std::{
    fs::DirEntry,
    path::Path,
    time::Duration,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{bail, Error as AnyError};
use blake2::{Blake2s256, Digest};
use byteorder::{BigEndian, ByteOrder};
use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Utc};
use id3::{Tag, TagLike, Version};
use symphonia_bundle_mp3::{MpaDecoder, MpaReader};
use symphonia_core::{
    codecs::{CodecParameters, Decoder, DecoderOptions, CODEC_TYPE_MP3},
    formats::{FormatOptions, FormatReader},
    io::{MediaSourceStream, MediaSourceStreamOptions, ReadOnlySource},
};

/// Filenames in `audio_blobs` are of the form `TITLE-HASH.mp3`. This is maximum number of bytes
/// allowed in `TITLE`. This MUST be less than 256.
const FILENAME_TITLE_MAXLEN: usize = 20;

/// Filenames in `audio_blobs` are of the form `TITLE-HASH.mp3`. This is the size of the hash
/// BEFORE it is encoded in zbase32.
const ARTICLE_HASH_BITLEN: u64 = 128;

/// Defines how we sanitize article filenames. This assumes we're on Windows (which is more
/// restrictive), doesn't truncate the filename (we do that ourselves), and replaces sanitized
/// characters with '_'
const SANITIZATION_OPTIONS: sanitize_filename::Options<'static> = sanitize_filename::Options {
    windows: true,
    truncate: false,
    replacement: "_",
};

/// Used in `truncate_to_bytes` to specify the byte encoding of the string to be truncated
pub(crate) enum StrEncoding {
    Utf8,
    Utf16,
}

/// Truncates a string to occupy at most `max_len` bytes, under the given encoding
///
/// Panics: if `max_len < 4`
pub(crate) fn truncate_to_bytes(string: &str, max_len: usize, encoding: StrEncoding) -> String {
    // A codepoint in UTF-8 is at most 4 bytes. So the smallest we can guarantee truncation to is 4
    // bytes.
    if max_len < 4 {
        panic!("Cannot guarantee a truncation with less than 4 bytes");
    }

    // Count the bytes of each character. We'll take the longest prefix of the string whose
    // encoding is below the length limit
    let mut byte_count = 0;
    string
        .chars()
        .take_while(|c| {
            let char_bytelen = match encoding {
                StrEncoding::Utf8 => c.len_utf8(),
                StrEncoding::Utf16 => 2 * c.len_utf16(),
            };

            // Take characters until we hit the length limit
            byte_count += char_bytelen;
            byte_count <= max_len
        })
        .collect()
}

/// Computes the zbase32 encoded hash of the given article. The output length is ARTICLE_HASH_LEN.
fn hash_article(ArticleTextSubmission { title, body }: &ArticleTextSubmission) -> String {
    // We will compute H(title_len || title || body)
    let mut h = Blake2s256::default();

    // Write the title length to a buffer
    let mut len_buf = [0u8; 8];
    BigEndian::write_u64(&mut len_buf, title.len().try_into().unwrap());

    // Compute H(title_len || title || body)
    h.update(len_buf);
    h.update(title);
    h.update(body);

    // Now get the hash, truncate to ARTICLE_HASH_BITLEN, and convert to zbase32
    let digest = h.finalize();
    zbase32::encode(&digest, ARTICLE_HASH_BITLEN)
}
/// Derives the unique ID of this article. It's of the form SHORTTITLE-HASH.mp3, where SHORTTITLE
/// is the sanitized, truncated title of the article
pub fn derive_article_id(article: &ArticleTextSubmission) -> String {
    let sanitized_title =
        sanitize_filename::sanitize_with_options(&article.title, SANITIZATION_OPTIONS);
    let truncated_title =
        truncate_to_bytes(&sanitized_title, FILENAME_TITLE_MAXLEN, StrEncoding::Utf8);
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
        let date = epoch_secs_to_datetime(added);
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

/// Converts seconds since epoch to UTC datetime
pub(crate) fn epoch_secs_to_datetime(secs: u64) -> DateTime<Utc> {
    let date = NaiveDateTime::from_timestamp(
        secs.try_into()
            .expect("it is 2038 and chrono still uses i64 for unix time"),
        0,
    );
    DateTime::<Utc>::from_utc(date, Utc)
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
        duration: None,
        source_url: None,
        datetime_added: last_modified_timestamp,
    };

    // Try to get the metadata from the ID3 tags
    if let Ok(tag) = Tag::read_from_path(path) {
        // Try to get the ID3 title, source URL (URL is in the Artist field), and duration
        meta.title = tag.title().unwrap_or(&meta.title).to_string();
        meta.source_url = tag.artist().map(str::to_string);
        meta.duration = tag.duration().map(|t| Duration::from_secs(t as u64));

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

/// Returns the true duration of an MP3 file. This is somewhat expensive, so it should only be
/// computed once, and cached in the metadata
pub(crate) fn get_mp3_duration(path: &std::path::PathBuf) -> Result<Duration, anyhow::Error> {
    // Make the file into an input stream
    let f = std::fs::File::open(&path)?;
    let src = ReadOnlySource::new(f);
    let media_src = MediaSourceStream::new(
        Box::new(src),
        MediaSourceStreamOptions {
            buffer_len: 1048576, // 1MB
        },
    );
    let mut mp3 = MpaReader::try_new(media_src, &FormatOptions::default())?;

    // Get the sample rate from the first packet. Also record the number of samples
    let mut decoder = MpaDecoder::try_new(
        &CodecParameters::default().for_codec(CODEC_TYPE_MP3),
        &DecoderOptions::default(),
    )
    .expect("could not construct basic MP3 decoder");
    let (sample_rate, mut num_samples) = {
        // Get the first packet and decode it for the sample rate
        let first_packet = mp3.next_packet()?;
        let sample_rate = decoder.decode(&first_packet).map(|buf| buf.spec().rate)?;
        (sample_rate, first_packet.dur)
    };
    // Let num_samples be the number of samples over *all* the packets
    while let Ok(p) = mp3.next_packet() {
        num_samples += p.dur;
    }
    // Compute the duration in seconds
    Ok(Duration::from_secs_f64(
        num_samples as f64 / sample_rate as f64,
    ))
}

#[test]
fn test_title_truncation() {
    let title = "Money Stuff: AMC’s APEs Might Stick Around";
    assert_eq!(
        truncate_to_bytes(title, FILENAME_TITLE_MAXLEN, StrEncoding::Utf8),
        "Money Stuff: AMC’s"
    );
}
