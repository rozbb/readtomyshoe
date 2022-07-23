//! Implements a barebones client to the Google Cloud TTS service

use anyhow::{anyhow, bail, Error};
use bytes::Bytes;
use futures::Future;
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

pub(crate) fn get_api_key() -> Result<String, Error> {
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
pub(crate) async fn tts_single(api_key: &str, req: &TtsRequest) -> Result<Bytes, Error> {
    let payload = req.into_json();

    if req.text.len() > MAX_CHARS_PER_REQUEST {
        bail!("TTS request is too long");
    }

    let client = reqwest::Client::new();
    let url = reqwest::Url::parse_with_params(GCP_TTS_API, &[("key", api_key)])?;

    let res = client
        .post(url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| Error::from(e).context("network failed"))?
        .error_for_status()
        .map_err(|e| Error::from(e).context("TTS API req failed"))?;

    let res_bytes = res.bytes().await?;
    let audio_response: AudioResponse = serde_json::from_slice(&res_bytes)?;
    let audio_blob = Bytes::from(base64::decode(audio_response.audio_content)?);

    Ok(audio_blob)
}

// Helper function. Computes the given futures in parallel, and collects its results. Code from
// https://stackoverflow.com/a/63437482
async fn join_parallel<T: Send + 'static>(
    futs: impl IntoIterator<Item = impl Future<Output = T> + Send + 'static>,
) -> Vec<T> {
    let tasks: Vec<_> = futs.into_iter().map(tokio::spawn).collect();
    // unwrap the Result because it is introduced by tokio::spawn()
    // and isn't something our caller can handle
    futures::future::join_all(tasks)
        .await
        .into_iter()
        .map(Result::unwrap)
        .collect()
}

/// Speaks text string. Returns an error if an error occurs in the Google Cloud API call.
pub(crate) async fn tts(
    api_key: &str,
    TtsRequest { text, use_wavenet }: TtsRequest,
) -> Result<Bytes, Error> {
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

    // Do the tasks in parallel and concat the resulting MP3 files. Fun fact: the concatenation of
    // MP3 files is itself a valid MP3 file.
    let mp3_blobs: Result<Vec<Bytes>, Error> = join_parallel(tts_tasks).await.into_iter().collect();
    let final_mp3 = mp3_blobs?.concat().into();
    Ok(final_mp3)
}

/// Splits the given str at all the specified indices. Also removes the first character of the
/// all but the first split, since it's just punctuation.
fn split_at_multi<'a>(mut text: &'a str, indices: &[usize]) -> Vec<&'a str> {
    let mut slices = Vec::new();
    let mut last_idx = 0;

    for &idx in indices {
        // Split at the specified point and save the slice
        let (slice, rest) = text.split_at(idx - last_idx);
        slices.push(slice);

        // Update the running slice and pos
        last_idx = idx;
        // Strip the leading punctuation. This happens after the first split. This shifts all the
        // indices down by 1
        //text = &rest[1..];
        text = rest;
    }

    // Push the remaining slice
    slices.push(text);
    slices
}

/// Attempts to break the given text into chunks of size at most `max_chunk_size`, making chunks as
/// large as possible. The only allowed break point is `delim` characters. This is best-effort,
/// meaning that there may be chunks returned which exceed `max_chunk_size`.
pub(crate) fn break_greedily_at_delim(text: &str, delim: char, max_chunk_size: usize) -> Vec<&str> {
    let mut breaks = Vec::new();

    // Find all the delimiters
    let delim_indices = text.match_indices(delim).map(|(i, _)| i);

    // Keep track of the last break that we haven't chosen
    let mut last_candidate_break = None;

    // Make the last breakpoint the EOF, otherwise we'd just be counting span size between delims
    // (leaving out the span between the final delim and EOF)
    let eof = text.len();
    for cur_break in delim_indices.chain(iter::once(eof)) {
        let last_break = breaks.last().unwrap_or(&0);

        if cur_break - last_break <= max_chunk_size {
            last_candidate_break = Some(cur_break);
        } else {
            // If we cannot break here, try to break at the last candidate. If there was none, then
            // we cannot break.
            match last_candidate_break {
                Some(b) => {
                    breaks.push(b);
                    last_candidate_break = Some(cur_break);
                }
                None => {
                    // This chunk is too big, but we can't do anything about it. Add a break right
                    // here (unless it's the end of the text)
                    if cur_break != eof {
                        breaks.push(cur_break);
                        // Clear the candidate break. Next break has to be after cur_break
                        last_candidate_break = None;
                    }
                }
            }
        }
    }

    split_at_multi(text, &breaks)
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
) -> Result<Vec<&'a str>, Error> {
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
fn break_english_text(text: &str, max_chunk_size: usize) -> Result<Vec<&str>, Error> {
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

    // Make sure the chunk sizes add up the the total text size
    assert_eq!(chunks.iter().map(|c| c.len()).sum::<usize>(), text.len());

    // Make sure the chunks are nonempty and don't exceed the chunk_size
    for chunk in chunks {
        assert!(chunk.len() > 0);
        assert!(chunk.len() <= chunk_size);
    }

    let short_text = "\
        Mr James Duffy lived in Chapelizod because he wished to live as far as possible from the \
        city of which he was a citizen and because he found all the other suburbs of Dublin mean, \
        modern and pretentious.\
    ";
    let chunks = break_english_text(short_text, chunk_size).unwrap();
    assert_eq!(chunks.len(), 1);
    // Make sure the chunk sizes add up the the total text size
    assert_eq!(
        chunks.iter().map(|c| c.len()).sum::<usize>(),
        short_text.len()
    );
}
