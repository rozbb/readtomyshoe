//! Implements a barebones client to the Google Cloud TTS service

use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use std::time::{self, UNIX_EPOCH};

use anyhow::{bail, Error};
use bytes::Bytes;
use reqwest::Error as ReqwestError;
use serde::{Deserialize, Serialize};

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

pub(crate) struct TtsRequest {
    /// The contents of the request
    pub text: String,
    /// Whether or not to use the expensive voices
    pub wavenet: bool,
}

impl TtsRequest {
    fn into_json(&self) -> serde_json::Value {
        let voice_name = if self.wavenet {
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

/// Translates text string of length at most MAX_CHARS_PER_REQUEST
pub(crate) async fn tts(api_key: &str, req: &TtsRequest) -> Result<Bytes, Error> {
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

pub(crate) fn get_api_key() -> Result<String, Error> {
    std::fs::read_to_string(API_KEY_FILE).map_err(|e| {
        anyhow::anyhow!(
            "Could not open API key file {API_KEY_FILE}. \
            Read the README for info about how to get an API key. {:?}",
            e,
        )
    })
}
