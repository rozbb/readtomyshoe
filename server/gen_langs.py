# This Python 3 script produces the data structures used in lang.rs. The script requires the API_KEY
# environment variable to be set to a Google Cloud API key that works with their Text to Speech
# service.

import http.client
from urllib.parse import urlencode
import os
import json

# A map from IETF BCP 47 language tag to their English description. Here, language codes are of the
# form LANG-COUNTRY where LANG is an ISO 639-1 language code, and COUNTRY is an ISO 3166-1 alpha-2
# country codes. The English description is the official English expansion of the individual codes,
# in the form "LANGUAGE (COUNTRY)"
ietf_code_to_english = {
    "af-ZA": "Afrikaans (South Africa)",
    "ar-XA": "Arabic",
    "bn-IN": "Bengali (India)",
    "bg-BG": "Bulgarian (Bulgaria)",
    "ca-ES": "Catalan (Spain)",
    "yue-HK": "Chinese (Hong Kong)",
    "cs-CZ": "Czech (Czech Republic)",
    "da-DK": "Danish (Denmark)",
    "nl-BE": "Dutch (Belgium)",
    "nl-NL": "Dutch (Netherlands)",
    "en-AU": "English (Australia)",
    "en-IN": "English (India)",
    "en-GB": "English (UK)",
    "en-US": "English (US)",
    "fil-PH": "Filipino (Philisppines)",
    "fi-FI": "Finnish (Finland)",
    "fr-CA": "French (Canada)",
    "fr-FR": "French (France)",
    "de-DE": "German (Germany)",
    "el-GR": "Greek (Greece)",
    "gu-IN": "Gujarati (India)",
    "hi-IN": "Hindi (India)",
    "hu-HU": "Hungarian (Hungary)",
    "is-IS": "Icelandic (Iceland)",
    "id-ID": "Indonesian (Indonesia)",
    "it-IT": "Italian (Italy)",
    "ja-JP": "Japanese (Japan)",
    "kn-IN": "Kannada (India)",
    "ko-KR": "Korean (South Korea)",
    "lv-LV": "Latvian (Latvia)",
    "ms-MY": "Malay (Malaysia)",
    "ml-IN": "Malayalam (India)",
    "mr-IN": "Marathi (India)",
    "cmn-CN": "Mandarin Chinese (China)",
    "cmn-TW": "Mandarin Chinese (Taiwan, Province of China)",
    "nb-NO": "Norwegian (Norway)",
    "pl-PL": "Polish (Poland)",
    "pt-BR": "Portuguese (Brazil)",
    "pt-PT": "Portuguese (Portugal)",
    "pa-IN": "Punjabi (India)",
    "ro-RO": "Romanian (Romania)",
    "ru-RU": "Russian (Russia)",
    "sr-RS": "Serbian (Serbia)",
    "sk-SK": "Slovak (Slovakia)",
    "es-ES": "Spanish (Spain)",
    "es-US": "Spanish (US)",
    "sv-SE": "Swedish (Sweden)",
    "ta-IN": "Tamil (India)",
    "te-IN": "Telugu (India)",
    "th-TH": "Thai (Thailand)",
    "tr-TR": "Turkish (Turkey)",
    "uk-UA": "Ukranian (Ukraine)",
    "vi-VN": "Vietnamese (Vietnam)",
}

# Map from IETF BCP 47 language tag to the code the Rust whatlang crate uses (a subset of ISO
# 639-3). The ones that are None are not supported by whatlang
ietf_code_to_whatlang = {
    "af-ZA": "Afr",
    "ar-XA": "Ara",
    "bn-IN": "Ben",
    "bg-BG": "Bul",
    "ca-ES": "Cat",
    "yue-HK": "Cmn",
    "cs-CZ": "Ces",
    "da-DK": "Dan",
    "nl-BE": "Nld",
    "nl-NL": "Nld",
    "en-AU": "Eng",
    "en-IN": "Eng",
    "en-GB": "Eng",
    "en-US": "Eng",
    "fil-PH": None,
    "fi-FI": "Fin",
    "fr-CA": "Fra",
    "fr-FR": "Fra",
    "de-DE": "Deu",
    "el-GR": "Ell",
    "gu-IN": "Guj",
    "hi-IN": "Hin",
    "hu-HU": "Hun",
    "is-IS": None,
    "id-ID": "Ind",
    "it-IT": "Ita",
    "ja-JP": "Jpn",
    "kn-IN": "Kan",
    "ko-KR": "Kor",
    "lv-LV": "Lav",
    "ms-MY": None,
    "ml-IN": "Mal",
    "mr-IN": "Mar",
    "cmn-CN": "Cmn",
    "cmn-TW": "Cmn",
    "nb-NO": None,
    "pl-PL": "Pol",
    "pt-BR": "Por",
    "pt-PT": "Por",
    "pa-IN": "Pan",
    "ro-RO": "Ron",
    "ru-RU": "Rus",
    "sr-RS": "Srp",
    "sk-SK": "Slk",
    "es-ES": "Spa",
    "es-US": "Spa",
    "sv-SE": "Swe",
    "ta-IN": "Tam",
    "te-IN": "Tel",
    "th-TH": "Tha",
    "tr-TR": "Tuk",
    "uk-UA": "Ukr",
    "vi-VN": "Vie",
}

# Get the API key from the env
api_key = os.environ["API_KEY"]

# Fetch the voices from the GCP API
conn = http.client.HTTPSConnection("texttospeech.googleapis.com", 443)
conn.request("GET", f"/v1beta1/voices?key={api_key}")
resp = conn.getresponse()
payload = resp.read()
voices = json.loads(payload)["voices"]

# We have to sort the voices a bit. It should probably be the case that the country with the largest
# number of speakers appears first for each language, so that the default option for that language
# covers the greatest number of people. People will still be able to pick their own region from the
# dropdown menu.
most_common_variants = ["en-US", "fr-FR", "es-US", "cmn-CN", "pt-BR", "nl-NL"]

# Collect all the voices, separating them by quality into Standard, Wavenet, and Neural2.

standard_voices = []
wavenet_voices = []
neural2_voices = []
# We need an override list because Neural2 en-GB voices still rank higher than Wavenet en-US voices
overrides = []

# Iterate backwards because I want en-US to come before en-GB, so that the search algo in lang.rs
# uses an American accent by default
for voice in voices:
    # Extract the IETF tag and voice unique identifier
    lang_code = voice["languageCodes"][0]
    id = voice["name"]

    # If the whatlang crate doesn't support the language, skip it
    whatlang = ietf_code_to_whatlang.get(lang_code)
    if not whatlang:
        continue

    # Construct the Rust fields for language, description, and pitch
    rust_lang = f"Lang::{ietf_code_to_whatlang[lang_code]}"
    english_desc = ietf_code_to_english[lang_code]
    ty = "VoiceType::LowPitch" if voice["ssmlGender"] == "MALE" else "VoiceType::HighPitch"

    # Construct the Rust struct, and tag it with the language
    voice_struct = f'GcpVoice {{ id: "{id}", english_desc: "{english_desc}", ty: {ty} }}'
    entry = f"\n({rust_lang}, {voice_struct})"

    # Get the appropriate list to append or prepend to
    list_to_modify = None
    if "Wavenet" in id:
        list_to_modify = wavenet_voices
    elif "Neural2" in id:
        list_to_modify = neural2_voices
    else:
        list_to_modify = standard_voices

    # If this language is the most common variant, put it at the beginning of the list
    if lang_code in most_common_variants:
        list_to_modify.insert(0, entry)
    else:
        list_to_modify.append(entry)

    # Include two US English Wavenet voices (one high pitch, one low) in the overrides. That means
    # it's considered before the British English Neural2 voices
    if id == "en-US-Wavenet-B" or id == "en-US-Wavenet-C":
        overrides.append(entry)


# Output the generated code
print("// The following code was generated by gen_langs.py")
print("")
print("const VOICE_OVERRIDES: &[(Lang, GcpVoice)] = &[", ",".join(overrides), "];")
print("")
print("const STANDARD_VOICES: &[(Lang, GcpVoice)] = &[", ",".join(standard_voices), "];")
print("")
print("const WAVENET_VOICES: &[(Lang, GcpVoice)] = &[", ",".join(wavenet_voices), "];")
print("")
print("const NEURAL2_VOICES: &[(Lang, GcpVoice)] = &[", ",".join(neural2_voices), "];")
