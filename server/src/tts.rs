//! Implements a barebones client to the Google Cloud TTS service

use anyhow::{anyhow, bail, Context, Error as AnyError};
use bytes::Bytes;
use serde::Deserialize;

use core::iter;

/// Path to the file that holds the Google Cloud API key
const API_KEY_FILE: &str = "gcp_api.key";

//const GCP_API_BASE: &str = "https://texttospeech.googleapis.com/v1";
const GCP_TTS_API: &str = "https://texttospeech.googleapis.com/v1beta1/text:synthesize";

// See https://cloud.google.com/text-to-speech/quotas
const MAX_CHARS_PER_REQUEST: usize = 5000;
const MAX_REQUESTS_PER_MINUTE: usize = 1000;

#[derive(Deserialize)]
struct AudioResponse<'a> {
    #[serde(borrow, rename = "audioContent")]
    audio_content: &'a str,
}

#[derive(Clone, Debug)]
pub(crate) struct TtsRequest {
    /// The contents of the request
    pub text: String,
    /// Whether or not to use the expensive voices
    pub use_wavenet: bool,
}

impl TtsRequest {
    fn into_json(&self) -> serde_json::Value {
        let voice_name = if self.use_wavenet {
            "en-US-Wavenet-C"
        } else {
            "en-US-Standard-C"
        };

        serde_json::json!({
            "input": {
                "text": self.text
            },
            "voice":{
                "languageCode":"en-US",
                "name": voice_name,
            },
            "audioConfig":{
                "audioEncoding": "MP3_64_KBPS",
                "sampleRateHertz": 48000
            }
        })
    }
}

pub(crate) fn get_api_key() -> Result<String, AnyError> {
    std::fs::read_to_string(API_KEY_FILE).map_err(|e| {
        anyhow!(
            "Could not open API key file {API_KEY_FILE}. \
            Read the README for info about how to get an API key. {:?}",
            e,
        )
    })
}

/// Speaks text string of length at most MAX_CHARS_PER_REQUEST. Returns an error if length exceeds,
/// or an error occurs in the Google Cloud API call.
pub(crate) async fn tts_single(api_key: &str, req: &TtsRequest) -> Result<Bytes, AnyError> {
    let payload = req.into_json();

    // The Google API has a hard upper limit on characters per request. The text breaking before
    // this point should ensure this limit is never exceeded
    if req.text.len() > MAX_CHARS_PER_REQUEST {
        bail!("TTS request is too long");
    }

    // Do the HTTP request
    let client = reqwest::Client::new();
    let url = reqwest::Url::parse_with_params(GCP_TTS_API, &[("key", api_key)])?;
    let res = client
        .post(url)
        .json(&payload)
        .send()
        .await
        .with_context(|| "Couldn't make TTS request")?
        .error_for_status()
        .with_context(|| "TTS request failed")?;

    // The resulting JSON response has our MP3 data
    let res_bytes = res.bytes().await?;
    let audio_response: AudioResponse = serde_json::from_slice(&res_bytes)?;
    let audio_blob = Bytes::from(base64::decode(audio_response.audio_content)?);

    Ok(audio_blob)
}

/// Speaks text string. Returns an error if an error occurs in the Google Cloud API call.
pub(crate) async fn tts(
    api_key: &str,
    TtsRequest { text, use_wavenet }: TtsRequest,
) -> Result<Bytes, AnyError> {
    let api_key_iter = core::iter::repeat(api_key);

    // Break up the TTS tasks into smaller ones of at most MAX_CHARS_PER_REQUEST
    let tts_tasks = break_english_text(&text, MAX_CHARS_PER_REQUEST)?
        .into_iter()
        .zip(api_key_iter)
        .map(|(slice, api_key)| {
            let slice = slice.to_string();
            let api_key = api_key.to_string();
            async move {
                let slice_req = TtsRequest {
                    text: slice,
                    use_wavenet,
                };
                tts_single(&api_key, &slice_req).await
            }
        });

    // Do the tasks in parallel. If one task fails, try_join_all will cancel the rest of them
    // immediately. This prevents us from wasting API calls.
    let mp3_blobs = futures::future::try_join_all(tts_tasks).await?;
    // Concat the resulting MP3 blobs. Fun fact: the concatenation of MP3 files is itself a valid
    // MP3 file.
    let final_mp3: Bytes = mp3_blobs.concat().into();

    Ok(final_mp3)
}

// Helper function that finds the next index i of the delimiter in the text such that txt[0, i]
// is below the chunk limit. If no such i is found, then the first occurance of the delimiter
// is returned (and text[0, i] is too big). If no delimiter occurs at all, txt.len() is
// returned.
fn next_break(text: &str, delim: char, max_chunk_size: usize) -> usize {
    // Keep track of the last break that keeps us under the chunk size limit
    let mut last_candidate_break = None;

    // Find all the delimiters
    let delim_indices = text.match_indices(delim).map(|(i, _)| i);

    // Make the last breakpoint the EOF, otherwise we'd just be counting span size between
    // delims (leaving out the span between the final delim and EOF)
    let eof = text.len();

    for cur_break in delim_indices.chain(iter::once(eof)) {
        if cur_break != eof {
            assert_eq!(text.split_at(cur_break).1.chars().next().unwrap(), delim);
        }
        if cur_break <= max_chunk_size {
            last_candidate_break = Some(cur_break);
        } else {
            // The current break puts us over the chunk limit. Split at the last break if
            // possible.
            return last_candidate_break.unwrap_or(cur_break);
        }
    }

    // If we made it to the end, then the text is now small enough to be a chunk in itself, and
    // we're done.
    return eof;
}

/// Attempts to break the given text into chunks of size at most `max_chunk_size`, making chunks as
/// large as possible. The only allowed break point is `delim` characters. This is best-effort,
/// meaning that there may be chunks returned which exceed `max_chunk_size`.
pub(crate) fn break_greedily_at_delim(
    mut text: &str,
    delim: char,
    max_chunk_size: usize,
) -> Vec<&str> {
    let mut chunks = Vec::new();

    while !text.is_empty() {
        // While the text is not fully chunked, find the next break and break it up.
        let b = next_break(text, delim, max_chunk_size);
        let (chunk, mut rest) = text.split_at(b);

        // Strip the leading punctuation off of the remainder of the text
        if rest.chars().nth(0) == Some(delim) {
            rest = &rest[1..];
        }

        // Save the chunk and truncate the text
        chunks.push(chunk);
        text = rest;
    }

    chunks
}

/// Attempts to break the given text into chunks of size at most `max_chunk_size`, making chunks as
/// large as possible. The delimiters given are used in decreasing order of preference. If the text
/// can be broken at just newlines, for example, that'd be preferred. But if there's a very large
/// paragraph, then we have to break at periods. And if there's a very long sentence, we have to
/// break on commas, etc.
fn break_greedily_at_delims<'a>(
    text: &'a str,
    max_chunk_size: usize,
    delims: &[char],
) -> Result<Vec<&'a str>, AnyError> {
    // Break the text into the f irst delim first
    let mut chunks = break_greedily_at_delim(text, delims[0], max_chunk_size);

    // If there are chunks greater than max_chunk_size, break them up. Use every delimiter if need
    // be.
    for &delim in &delims[1..] {
        chunks = chunks
            .into_iter()
            .flat_map(|chunk| {
                if chunk.len() > max_chunk_size {
                    break_greedily_at_delim(&chunk, delim, max_chunk_size)
                } else {
                    vec![chunk]
                }
            })
            .collect();
    }

    // Now check that everything was broken into sufficiently small pieces
    for chunk in &chunks {
        if chunk.len() > max_chunk_size {
            bail!("Couldn't break chunk {:?}", chunk);
        }
    }

    Ok(chunks)
}

/// Breaks English text into bounded-size chunks by newlines, then (if necessary) colons,
/// then periods, then commas.
fn break_english_text(text: &str, max_chunk_size: usize) -> Result<Vec<&str>, AnyError> {
    break_greedily_at_delims(text, max_chunk_size, &['\n', ':', '.', ','])
}

#[test]
fn text_breaking() {
    let text = "\
        Mr James Duffy lived in Chapelizod because he wished to live as far as possible from the \
        city of which he was a citizen and because he found all the other suburbs of Dublin mean, \
        modern and pretentious.
        He lived in an old sombre house and from his windows he could look into the disused \
        distillery or upwards along the shallow river on which Dublin is built. The lofty walls \
        of his uncarpeted room were free from pictures. He had himself bought every article of \
        furniture in the room: a black iron bedstead, an iron washstand, four cane chairs, a \
        clothes-rack, a coal-scuttle, a fender and irons and a square table on which lay a double \
        desk. A bookcase had been made in an alcove by means of shelves of white wood. The bed \
        was clothed with white bedclothes and a black and scarlet rug covered the foot. A little \
        hand-mirror hung above the washstand and during the day a white-shaded lamp stood as the \
        sole ornament of the mantelpiece. The books on the white wooden shelves were arranged \
        from below upwards according to bulk. A complete Wordsworth stood at one end of the \
        lowest shelf and a copy of the Maynooth Catechism, sewn into the cloth cover of a \
        notebook, stood at one end of the top shelf. Writing materials were always on the desk. \
        In the desk lay a manuscript translation of Hauptmann’s Michael Kramer, the stage \
        directions of which were written in purple ink, and a little sheaf of papers held \
        together by a brass pin. In these sheets a sentence was inscribed from time to time and, \
        in an ironical moment, the headline of an advertisement for Bile Beans had been pasted on \
        to the first sheet. On lifting the lid of the desk a faint fragrance escaped—the \
        fragrance of new cedarwood pencils or of a bottle of gum or of an overripe apple which \
        might have been left there and forgotten.\
    ";
    let chunk_size = 220;
    let chunks = break_english_text(text, chunk_size).unwrap();

    // Make sure the chunk sizes add up the the total text size, minus the delimiters which were
    // deleted (there's chunks.len() - 1 deleted delimiters).
    assert_eq!(
        chunks.iter().map(|c| c.len()).sum::<usize>(),
        text.len() - (chunks.len() - 1)
    );

    // Make sure the chunks are nontrivial and don't exceed the chunk_size
    for chunk in chunks {
        assert!(chunk.len() > 1);
        assert!(chunk.len() <= chunk_size);
    }

    //
    // Test for a short text sample
    //

    let text = "\
        Mr James Duffy lived in Chapelizod because he wished to live as far as possible from the \
        city of which he was a citizen and because he found all the other suburbs of Dublin mean, \
        modern and pretentious.\
    ";
    let chunks = break_english_text(text, chunk_size).unwrap();
    // Make sure there's just one chunk and it's the length of the whole text
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].len(), text.len());
}
