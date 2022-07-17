use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use web_sys::{Blob, BlobPropertyBag};

const MP3_MIME_TYPE: &str = "audio/mp3";

/// Converts bytes to a blob with the given MIME type
pub fn bytes_to_mp3_blob(bytes: &[u8]) -> Blob {
    let arr = js_sys::Uint8Array::from(bytes);

    // A blob is made from an array of arrays. So construct [bytes] and use that.
    let parts = js_sys::Array::new();
    parts.set(0, JsValue::from(arr));
    Blob::new_with_u8_array_sequence_and_options(
        &parts,
        BlobPropertyBag::new().type_(MP3_MIME_TYPE),
    )
    .unwrap()
}

/// Runs the given closure after `secs` seconds
pub fn run_after_delay(closure: &Closure<dyn Fn()>, secs: i32) {
    let win = gloo_utils::window();
    let func = closure.as_ref().unchecked_ref();
    if let Err(e) = win.set_timeout_with_callback_and_timeout_and_arguments_0(func, secs) {
        tracing::error!("Could not set timeout with callback: {:?}", e);
    }
}
