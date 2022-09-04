use anyhow::Error as AnyError;
use gloo_net::http::Response;
use js_sys::Uint8Array;
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{
    Blob, BlobPropertyBag, ReadableStream, ReadableStreamDefaultController,
    ReadableStreamDefaultReader,
};

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

// Wraps an HTTP response so that the given function runs every time a chunk is read. This pattern
// was taken from
// https://github.com/AnthumChris/fetch-progress-indicators/blob/efaaaf073bc6927a803e5963a92ba9b11a585cc0/fetch-basic/supported-browser.js
pub(crate) fn response_with_read_callback<F: 'static + FnMut(Uint8Array)>(
    resp: Response,
    mut callback: F,
) -> Result<Response, AnyError> {
    // Extract the reader from the given response
    let reader: ReadableStreamDefaultReader = resp
        .body()
        .ok_or(AnyError::msg("could not get repsonse body"))?
        .get_reader()
        .unchecked_into();

    // Define the start() function for our custom ReadableStream. This is all that's necessary to
    // define.
    let start_cb = Closure::once(move |controller: ReadableStreamDefaultController| {
        spawn_local(async move {
            loop {
                // Every read() operation returns a Promise to (done, chunk), where `done` marks
                // that the stream has terminated and `chunk` is the incoming bytes, if `done` is
                // not set.
                let chunk_promise = reader.read();
                let chunk = match JsFuture::from(chunk_promise).await {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = ReadableStreamDefaultController::error_with_e(&controller, &e);
                        break;
                    }
                };

                // True iff this stream is finished
                let done = js_sys::Reflect::get(&chunk, &JsValue::from_str("done")).unwrap()
                    == JsValue::TRUE;

                // If the stream is done, close it out and return
                if done {
                    let _ = ReadableStreamDefaultController::close(&controller);
                    break;
                } else {
                    // If there's another chunk to process get it and append it to the queue
                    let chunk = js_sys::Reflect::get(&chunk, &JsValue::from_str("value")).unwrap();
                    match controller.enqueue_with_chunk(&chunk) {
                        // If appending the chunk fails, abort
                        Err(e) => {
                            tracing::error!("could not enqueue chunk: {:?}", e);
                            break;
                        }
                        _ => (),
                    }

                    // Treat the chunk as an array and call the callback
                    let chunk = chunk.unchecked_into::<Uint8Array>();
                    callback(chunk);
                }
            }
        })
    });

    // Make an object { "start": start_cb }
    let readable_stream_callbacks = js_sys::Object::default();
    js_sys::Reflect::set(
        readable_stream_callbacks.as_ref(),
        &JsValue::from_str("start"),
        start_cb.as_ref(),
    )
    .unwrap();

    // Make a new ReadableStream with the above callback object
    let readable_stream =
        ReadableStream::new_with_underlying_source(&readable_stream_callbacks).unwrap();

    // Finally make a Response out of the above ReadableStream
    let resp = {
        let raw = web_sys::Response::new_with_opt_readable_stream(Some(&readable_stream)).unwrap();
        gloo_net::http::Response::from_raw(raw)
    };

    Ok(resp)
}
