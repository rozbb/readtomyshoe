//! Implements a barebones client to the Google Cloud TTS service

use anyhow::{bail, Error};
use bytes::Bytes;
use futures::Future;
use serde::Deserialize;

/// Path to the file that holds the Google Cloud API key
const API_KEY_FILE: &str = "gcp_api.key";

//const GCP_API_BASE: &str = "https://texttospeech.googleapis.com/v1";
const GCP_TTS_API: &str = "https://texttospeech.googleapis.com/v1beta1/text:synthesize";

// See https://cloud.google.com/text-to-speech/quotas
const MAX_CHARS_PER_REQUEST: usize = 5000;
const MAX_REQUESTS_PER_MINUTE: usize = 1000;
const MAX_CHARS_PER_MINUTE: usize = 500000;

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
        anyhow::anyhow!(
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
    let api_key_iter = core::iter::repeat(api_key.to_string());

    // Break up the TTS tasks into smaller ones of at most MAX_CHARS_PER_REQUEST
    let tts_tasks = break_english_text(&text).into_iter().zip(api_key_iter).map(
        |(slice, api_key)| async move {
            let slice_req = TtsRequest {
                text: slice,
                use_wavenet,
            };
            tts_single(&api_key, &slice_req).await
        },
    );

    // Do the tasks in parallel and concat the resulting MP3 files. Fun fact: the concatenation of
    // MP3 files is itself a valid MP3 file.
    let mp3_blobs: Result<Vec<Bytes>, Error> = join_parallel(tts_tasks).await.into_iter().collect();
    let final_mp3 = mp3_blobs?.concat().into();
    Ok(final_mp3)
}

/// Splits the given str at all the specified indices
fn split_at_multi<'a>(mut text: &'a str, indices: &[usize]) -> Vec<String> {
    tracing::debug!("split_at_multi with indices {:?}", indices);
    let mut slices = Vec::new();
    let mut last_idx = 0;

    for &idx in indices {
        // Split at the specified point and save the slice
        let (slice, rest) = text.split_at(idx - last_idx);
        slices.push(slice.to_string());

        // Update the running slice and pos
        last_idx = idx;
        text = rest;
    }

    // Push the remaining slice
    slices.push(text.to_string());
    slices
}

/// Breaks the given text into at most MAX_CHARS_PER_REQUEST-sized chunks
pub(crate) fn break_english_text(text: &str) -> Vec<String> {
    let mut breaks = Vec::new();

    // First pass is break on newlines if possible

    // Find all the newlines
    let newline_indices = text.match_indices("\n").map(|(i, _)| i);
    // Keep track of the last break that puts us below the max char limiit
    let mut candidate_break = None;

    for nl in newline_indices {
        let last_break = breaks.last().unwrap_or(&0);

        // If we could reasonably break here, save it as a candidate
        if nl - last_break <= MAX_CHARS_PER_REQUEST {
            candidate_break = Some(nl);
        } else {
            // If we cannot break here, try to break at the last candidate. If there was none, then
            // we cannot break.
            match candidate_break {
                Some(b) => breaks.push(b),
                None => panic!("Could not find a sufficiently short paragraph to break"),
            }
        }
    }

    split_at_multi(text, &breaks)
}
