use crate::queue_view::{CachedArticle, CachedArticleHandle};

use std::sync::Arc;

use anyhow::{bail, Error as AnyError};
use gloo_utils::window;
use serde::de::DeserializeOwned;
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{
    Blob, BlobPropertyBag, Event, IdbDatabase, IdbObjectStoreParameters, IdbTransactionMode,
};

const SERVICE_WORKER_PATH: &str = "/service_worker.js";

const DB_NAME: &str = "readtomyshoe";
const DB_VERSION: u32 = 1;

/// Name for the table that holds article information
const ARTICLES_TABLE: &str = "articles";
/// Name for the table that holds current positions in playback as well as ordering
const POS_TABLE: &str = "pos";

/// Registers service_worker.js to do all the caching for this site. See service_worker.js for more
/// details.
pub fn register_service_worker() {
    // Get a ServiceWorkerContainer
    let sw_container = gloo_utils::window().navigator().service_worker();

    // Just point at the SERVICE_WORKER_PATH and let the JS file handle the rest
    spawn_local(async move {
        let reg_promise = sw_container.register(SERVICE_WORKER_PATH);
        let res = JsFuture::from(reg_promise).await;
        tracing::debug!("Registered service worker");

        if let Err(e) = res {
            tracing::error!("Error registering service worker: {:?}", e);
        }
    });
}

/// A helper function for methods that return JsValue as an error type
fn wrap_jserror(context_str: &'static str, v: JsValue) -> AnyError {
    AnyError::msg(format!("{:?}", v)).context(context_str)
}

/// Returns a handle to the global IndexedDB object for our app
pub(crate) async fn get_db() -> Result<IdbDatabase, AnyError> {
    let factory = window()
        .indexed_db()
        .map_err(|e| wrap_jserror("couldn't get db factory", e))?
        .unwrap();
    let db_req = factory
        .open_with_u32(DB_NAME, DB_VERSION)
        .map_err(|e| wrap_jserror("couldn't open db", e))?;

    // Make conditional vars to notify of the four possible events for an IdbOpenDbRequest:
    // success, error, upgradeneeded, and blocked
    let success_var = Arc::new(tokio::sync::Notify::new());
    let success_var2 = success_var.clone();
    let blocked_var = Arc::new(tokio::sync::Notify::new());
    let blocked_var2 = blocked_var.clone();
    let upgradeneeded_var = Arc::new(tokio::sync::Notify::new());
    let upgradeneeded_var2 = upgradeneeded_var.clone();
    let (error_tx, mut error_rx) = tokio::sync::mpsc::channel(1);

    // Set callbacks for the above events. Error has to send the error message over a channel. The
    // rest are just notifiers.
    let success_cb: Closure<dyn Fn(Event)> = Closure::new(move |_| success_var2.notify_one());
    let upgradeneeded_cb: Closure<dyn Fn(Event)> =
        Closure::new(move |_| upgradeneeded_var2.notify_one());
    let blocked_cb: Closure<dyn Fn(Event)> = Closure::new(move |_| blocked_var2.notify_one());
    let error_cb: Closure<dyn Fn(Event)> = Closure::new(move |e| {
        let error_tx2 = error_tx.clone();
        spawn_local(async move {
            // Send the error. We unwrap here. Something probably went very wrong if async channels
            // stop working
            error_tx2.send(e).await.unwrap();
        })
    });
    db_req.set_onsuccess(Some(success_cb.as_ref().unchecked_ref()));
    db_req.set_onerror(Some(error_cb.as_ref().unchecked_ref()));
    db_req.set_onblocked(Some(blocked_cb.as_ref().unchecked_ref()));
    db_req.set_onupgradeneeded(Some(upgradeneeded_cb.as_ref().unchecked_ref()));

    // Wait for an event to trigger, and cancel the remaining branches
    tokio::select! {
        _ = success_var.notified() => {
            let db = db_req
                .result()
                .map_err(|e| wrap_jserror("couldn't get db from IdbOpenDbRequest", e))?
                .into();
            Ok(db)
        },
        e = error_rx.recv() => {
            bail!("Error opening database {DB_NAME} v{DB_VERSION}: {:?}", e)
        }

        // Upgradeneeded happens the first time the DB is created. In this case, we need to
        // initialize the object stores
        _ = upgradeneeded_var.notified() => {
            let db = db_req
                .result()
                .map_err(|e| wrap_jserror("couldn't get db from IdbOpenDbRequest", e))?
                .into();
            initialize_db(&db).await?;
            Ok(db)
        }
        _ = blocked_var.notified() => {
            bail!("Error opening database {DB_NAME} v{DB_VERSION}: blocked");
        }
    }
}

/// Initializes the database with two object stores:
///     articles - Stores CachedArticle objects
///     pos - Stores a single CachedArticlePosition object. This contains global state about
///           the order of the articles in the queue, the current article being played, and the
///           current timestamp
async fn initialize_db(db: &IdbDatabase) -> Result<(), AnyError> {
    tracing::info!("Initializing DB");

    // The articles table is keyed by title
    let mut articles_params = IdbObjectStoreParameters::new();
    articles_params
        .auto_increment(false)
        .key_path(Some(&JsValue::from_str("title")));

    // The global position object will always reside at key 0
    let mut pos_params = IdbObjectStoreParameters::new();
    pos_params.auto_increment(false).key_path(None);

    // This will error if "queue" already exists! TODO: Handle this before trying to do a database
    // schema update.
    db.create_object_store_with_optional_parameters(ARTICLES_TABLE, &articles_params)
        .map_err(|e| wrap_jserror("couldn't make articles table", e))?;
    db.create_object_store_with_optional_parameters(POS_TABLE, &pos_params)
        .map_err(|e| wrap_jserror("couldn't make pos table", e))?;

    Ok(())
}

/// Puts the value in the given table and returns the key to it
pub(crate) async fn table_put(table_name: &str, val: &JsValue) -> Result<String, AnyError> {
    // Get the articles object store
    let table = get_db()
        .await?
        .transaction_with_str_and_mode(table_name, IdbTransactionMode::Readwrite)
        .map_err(|e| wrap_jserror("couldn't start transaction", e))?
        .object_store(table_name)
        .map_err(|e| wrap_jserror("couldn't get object store from transaction", e))?;

    tracing::info!("Got DB handle");

    // Request a put() operation on the table
    let req = table
        .put(val)
        .map_err(|e| wrap_jserror("couldn't save value to table", e))?;

    // Now handle the outcomes of the put. Make channels to indicate success or failure
    let success_var = Arc::new(tokio::sync::Notify::new());
    let success_var2 = success_var.clone();
    let (error_tx, mut error_rx) = tokio::sync::mpsc::channel(1);

    // Set callbacks for the above events. Error has to send the error message over a channel
    let success_cb: Closure<dyn Fn(Event)> = Closure::new(move |_| success_var2.notify_one());
    let error_cb: Closure<dyn Fn(Event)> = Closure::new(move |e| {
        let error_tx2 = error_tx.clone();
        spawn_local(async move {
            // Send the error. We unwrap here. Something probably went very wrong if async channels
            // stop working
            error_tx2.send(e).await.unwrap();
        })
    });
    req.set_onsuccess(Some(success_cb.as_ref().unchecked_ref()));
    req.set_onerror(Some(error_cb.as_ref().unchecked_ref()));

    // Wait for an event to trigger, and cancel the remaining branches
    tokio::select! {
        _ = success_var.notified() => {
            tracing::info!("Successfully put {:?}", val);

            // Get the key of the article we just pushed
            let key: String = req
                .result()
                .map_err(|e| wrap_jserror("couldn't get key from IdbRequest", e))?
                .as_string().unwrap();
            Ok(key)
        },
        e = error_rx.recv() => {
            bail!("Error writing to {}: {:?}", table_name, e);
        }
    }
}

/// Puts the value in the given table and returns the key to it
pub(crate) async fn table_delete(table_name: &str, key: &str) -> Result<(), AnyError> {
    // Get the articles object store
    let table = get_db()
        .await?
        .transaction_with_str_and_mode(table_name, IdbTransactionMode::Readwrite)
        .map_err(|e| wrap_jserror("couldn't start transaction", e))?
        .object_store(table_name)
        .map_err(|e| wrap_jserror("couldn't get object store from transaction", e))?;

    tracing::info!("Got DB handle");

    // Request a put() operation on the table
    let req = table
        .delete(&JsValue::from_str(key))
        .map_err(|e| wrap_jserror("couldn't save value to table", e))?;

    // Now handle the outcomes of the put. Make channels to indicate success or failure
    let success_var = Arc::new(tokio::sync::Notify::new());
    let success_var2 = success_var.clone();
    let (error_tx, mut error_rx) = tokio::sync::mpsc::channel(1);

    // Set callbacks for the above events. Error has to send the error message over a channel
    let success_cb: Closure<dyn Fn(Event)> = Closure::new(move |_| success_var2.notify_one());
    let error_cb: Closure<dyn Fn(Event)> = Closure::new(move |e| {
        let error_tx2 = error_tx.clone();
        spawn_local(async move {
            // Send the error. We unwrap here. Something probably went very wrong if async channels
            // stop working
            error_tx2.send(e).await.unwrap();
        })
    });
    req.set_onsuccess(Some(success_cb.as_ref().unchecked_ref()));
    req.set_onerror(Some(error_cb.as_ref().unchecked_ref()));

    // Wait for an event to trigger, and cancel the remaining branches
    tokio::select! {
        _ = success_var.notified() => {
            tracing::info!("Successfully deleted {:?}", key);

            // Get the key of the article we just pushed
            Ok(())
        },
        e = error_rx.recv() => {
            bail!("Error writing to {}: {:?}", table_name, e);
        }
    }
}

/// Gets the value in the given table at the given key
pub(crate) async fn table_get(table_name: &str, key: &str) -> Result<JsValue, AnyError> {
    // Get the articles object store
    let table = get_db()
        .await?
        .transaction_with_str_and_mode(table_name, IdbTransactionMode::Readonly)
        .map_err(|e| wrap_jserror("couldn't start transaction", e))?
        .object_store(table_name)
        .map_err(|e| wrap_jserror("couldn't get object store from transaction", e))?;

    tracing::info!("Got DB handle");

    // Request a put() operation on the table
    let req = table
        .get(&JsValue::from_str(key))
        .map_err(|e| wrap_jserror("couldn't get from table", e))?;

    // Now handle the outcomes of the get. Make channels to indicate success or failure
    let success_var = Arc::new(tokio::sync::Notify::new());
    let success_var2 = success_var.clone();
    let (error_tx, mut error_rx) = tokio::sync::mpsc::channel(1);

    // Set callbacks for the above events. Error has to send the error message over a channel
    let success_cb: Closure<dyn Fn(Event)> = Closure::new(move |_| success_var2.notify_one());
    let error_cb: Closure<dyn Fn(Event)> = Closure::new(move |e| {
        let error_tx2 = error_tx.clone();
        spawn_local(async move {
            // Send the error. We unwrap here. Something probably went very wrong if async channels
            // stop working
            error_tx2.send(e).await.unwrap();
        })
    });
    req.set_onsuccess(Some(success_cb.as_ref().unchecked_ref()));
    req.set_onerror(Some(error_cb.as_ref().unchecked_ref()));

    // Wait for an event to trigger, and cancel the remaining branches
    tokio::select! {
        _ = success_var.notified() => {
            tracing::info!("Succesfully got {}", key);

            // Deserialize the get result
            req
                .result()
                //.map_err(|e| wrap_jserror("couldn't get key from IdbRequest", e))?.into_serde()
                .map_err(|e| wrap_jserror("couldn't get key from IdbRequest", e))
        },
        e = error_rx.recv() => {
            bail!("Error getting {} from {}: {:?}", key, table_name, e);
        }
    }
}

/// Gets all the keys from the given table
pub(crate) async fn table_get_keys(table_name: &str) -> Result<Vec<CachedArticleHandle>, AnyError> {
    // Get the articles object store
    let table = get_db()
        .await?
        .transaction_with_str_and_mode(table_name, IdbTransactionMode::Readonly)
        .map_err(|e| wrap_jserror("couldn't start transaction", e))?
        .object_store(table_name)
        .map_err(|e| wrap_jserror("couldn't get object store from transaction", e))?;

    tracing::info!("Got DB handle");

    // Request a put() operation on the table
    let req = table
        .get_all_keys()
        .map_err(|e| wrap_jserror("couldn't get from table", e))?;

    // Now handle the outcomes of the get. Make channels to indicate success or failure
    let success_var = Arc::new(tokio::sync::Notify::new());
    let success_var2 = success_var.clone();
    let (error_tx, mut error_rx) = tokio::sync::mpsc::channel(1);

    // Set callbacks for the above events. Error has to send the error message over a channel
    let success_cb: Closure<dyn Fn(Event)> = Closure::new(move |_| success_var2.notify_one());
    let error_cb: Closure<dyn Fn(Event)> = Closure::new(move |e| {
        let error_tx2 = error_tx.clone();
        spawn_local(async move {
            // Send the error. We unwrap here. Something probably went very wrong if async channels
            // stop working
            error_tx2.send(e).await.unwrap();
        })
    });
    req.set_onsuccess(Some(success_cb.as_ref().unchecked_ref()));
    req.set_onerror(Some(error_cb.as_ref().unchecked_ref()));

    // Wait for an event to trigger, and cancel the remaining branches
    tokio::select! {
        _ = success_var.notified() => {
            tracing::info!("Succesfully got all keys");

            // Deserialize the get result
            let key_arr = req
                .result()
                .map_err(|e| wrap_jserror("couldn't get key from IdbRequest", e))?
                .dyn_into::<js_sys::Array>()
                .unwrap();

            let mut keys: Vec<CachedArticleHandle> = Vec::new();
            key_arr.for_each(&mut |v: JsValue, _, _| {
                keys.push(CachedArticleHandle(v.as_string().unwrap()));
            });

            Ok(keys)
        },
        e = error_rx.recv() => {
            bail!("Error getting all keys from {}: {:?}", table_name, e);
        }
    }
}

/// Saves the given article to IndexedDB
pub(crate) async fn save_article(article: &CachedArticle) -> Result<CachedArticleHandle, AnyError> {
    // Request a put() operation on the table
    //let serialized_article = JsValue::from_serde(&article)?;

    let serialized_article = js_sys::Object::new();
    // Set the title
    js_sys::Reflect::set(
        &serialized_article,
        &JsValue::from_str("title"),
        &JsValue::from_str(&article.title),
    )
    .unwrap();

    // Set the blob
    let blob = {
        let bytes = js_sys::Uint8Array::from(article.audio_blob.as_slice());
        // A blob is made from an array of arrays. So construct [bytes] and use that.
        let parts = js_sys::Array::new();
        parts.set(0, JsValue::from(bytes));
        Blob::new_with_u8_array_sequence_and_options(
            &parts,
            BlobPropertyBag::new().type_("audio/mp3"),
        )
        .unwrap()
    };
    js_sys::Reflect::set(&serialized_article, &JsValue::from_str("audio_blob"), &blob).unwrap();

    table_put(ARTICLES_TABLE, &serialized_article)
        .await
        .map(CachedArticleHandle)
}

pub(crate) async fn load_article(handle: &CachedArticleHandle) -> Result<CachedArticle, AnyError> {
    // Request a get() operation on the table
    let serialized_article = table_get(ARTICLES_TABLE, &handle.0).await?;
    let title = js_sys::Reflect::get(&serialized_article, &JsValue::from_str("title"))
        .unwrap()
        .as_string()
        .unwrap();
    let js_blob: Blob = js_sys::Reflect::get(&serialized_article, &JsValue::from_str("audio_blob"))
        .unwrap()
        .dyn_into()
        .unwrap();
    let array_buf = JsFuture::from(js_blob.array_buffer()).await.unwrap();
    let audio_blob = js_sys::Uint8Array::new(&array_buf).to_vec();

    Ok(CachedArticle { title, audio_blob })
}

pub(crate) async fn delete_article(handle: &CachedArticleHandle) -> Result<(), AnyError> {
    table_delete(ARTICLES_TABLE, &handle.0).await
}

pub(crate) async fn load_handles() -> Result<Vec<CachedArticleHandle>, AnyError> {
    table_get_keys(ARTICLES_TABLE).await
}
