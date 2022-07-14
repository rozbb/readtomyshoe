use crate::{
    player_view::{ArticleState, PlayerState},
    queue_view::{ArticleId, CachedArticle, QueueEntry},
};

use std::sync::Arc;

use anyhow::{anyhow, bail, Error as AnyError};
use gloo_utils::window;
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{
    Blob, BlobPropertyBag, Event, IdbDatabase, IdbObjectStore, IdbObjectStoreParameters,
    IdbRequest, IdbTransactionMode, RegistrationOptions,
};

// TODO Fixme: This path is only valid in production mode
const SERVICE_WORKER_PATH: &str = "/assets/service-worker.js";

const DB_NAME: &str = "readtomyshoe";
const DB_VERSION: u32 = 1;

/// Name for the table that holds article information
const ARTICLES_TABLE: &str = "articles";

/// Name for the table that holds the current playback position for every article in ARTICLES_TABLE
const ARTICLE_STATE_TABLE: &str = "article-states";

/// Name for the table that holds the current player state, including the currently playing
/// article, and playback speed
const PLAYER_STATE_TABLE: &str = "player-state";

/// Registers service_worker.js to do all the caching for this site. See service_worker.js for more
/// details.
pub fn register_service_worker() {
    // Get a ServiceWorkerContainer
    let sw_container = gloo_utils::window().navigator().service_worker();

    // There's plenty of reasons we can't get a service worker. For one,
    // we can only register a service worker in a "seucre context", i.e., if the website is running
    // on localhost or via HTTPS. Also, you can't register a service worker in Firefox in a private
    // browsing window. So to catch all of these, just return early if we can't get one.
    if sw_container.is_undefined() {
        tracing::warn!("Could not register a ServiceWorker. Offline mode is unavailable");
        return;
    }

    // Just point at the SERVICE_WORKER_PATH and let the JS file handle the rest
    spawn_local(async move {
        // The service worker lives in /assets/, so make sure it has access to the whole site
        let mut reg_options = RegistrationOptions::new();
        reg_options.scope("./");

        // Register
        let reg_promise = sw_container.register_with_options(SERVICE_WORKER_PATH, &reg_options);
        let res = JsFuture::from(reg_promise).await;

        if let Err(e) = res {
            tracing::error!("Error registering service worker: {:?}", e);
        } else {
            tracing::debug!("Registered service worker");
        }
    });
}

fn jsvalue_to_str(v: JsValue) -> String {
    js_sys::JSON::stringify(&v)
        .ok()
        .and_then(|s| s.as_string())
        .unwrap_or("[JS error not displayable]".to_string())
}

/// A helper function for methods that return JsValue as an error type
fn wrap_jserror(context_str: &'static str, v: JsValue) -> AnyError {
    anyhow!("{context_str}: {:?}", v)
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
            let e: JsValue = e.map(Into::into).unwrap_or(JsValue::NULL);
            let event_str = jsvalue_to_str(e);
            let err_str = format!("Error opening database {DB_NAME} v{DB_VERSION}: {}", event_str);
            Err(AnyError::msg(err_str))
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
    tracing::trace!("Initializing DB");

    // The articles table is keyed by ID
    let mut articles_params = IdbObjectStoreParameters::new();
    articles_params
        .auto_increment(false)
        .key_path(Some(&JsValue::from_str("id")));

    // The article-states table is keyed by ID
    let mut article_states_params = IdbObjectStoreParameters::new();
    article_states_params
        .auto_increment(false)
        .key_path(Some(&JsValue::from_str("id")));

    // The global position object will always reside at key 0
    let mut pos_params = IdbObjectStoreParameters::new();
    pos_params.auto_increment(false).key_path(None);

    // This will error if "queue" already exists! TODO: Handle this before trying to do a database
    // schema update.
    db.create_object_store_with_optional_parameters(ARTICLES_TABLE, &articles_params)
        .map_err(|e| wrap_jserror("couldn't make articles table", e))?;
    db.create_object_store_with_optional_parameters(ARTICLE_STATE_TABLE, &articles_params)
        .map_err(|e| wrap_jserror("couldn't make article states table", e))?;
    db.create_object_store_with_optional_parameters(PLAYER_STATE_TABLE, &pos_params)
        .map_err(|e| wrap_jserror("couldn't make pos table", e))?;

    Ok(())
}

/// A helper function that runs whatever you want on the specified table. `write` indicates whether
/// `table_op` needs write access to the table. `table_op` takes a table and does some operations
/// that result in an `IdbRequest`. Once that request is finished, returns the resulting `JsValue`.
pub(crate) async fn access_db<F>(
    table_name: &str,
    write: bool,
    table_op: F,
) -> Result<JsValue, AnyError>
where
    F: FnOnce(&IdbObjectStore) -> Result<IdbRequest, AnyError>,
{
    let transaction_mode = if write {
        IdbTransactionMode::Readwrite
    } else {
        IdbTransactionMode::Readonly
    };

    // Get the articles object store
    let table = get_db()
        .await?
        .transaction_with_str_and_mode(table_name, transaction_mode)
        .map_err(|e| wrap_jserror("couldn't start transaction", e))?
        .object_store(table_name)
        .map_err(|e| wrap_jserror("couldn't get object store from transaction", e))?;

    tracing::trace!("Got DB handle");

    // Do the operation on the table
    let req = table_op(&table)?;

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
            req.result().map_err(|e| wrap_jserror("DB succeeded but returned error", e))
        },
        e = error_rx.recv() => {
            bail!("{:?}", e)
        }
    }
}

/// Puts the value in the given table and returns the key to it
pub(crate) async fn table_put(table_name: &str, val: &JsValue) -> Result<String, AnyError> {
    // Request a put() operation on the table
    let table_op = |table: &IdbObjectStore| {
        table
            .put(val)
            .map_err(|e| wrap_jserror("couldn't insert value to table", e))
    };

    // Run the operation
    match access_db(table_name, true, table_op).await {
        Ok(key) => {
            tracing::trace!("Successfully put {:?}", key);
            Ok(key.as_string().unwrap())
        }
        Err(e) => Err(anyhow!("Error inserting into {}: {}", table_name, e)),
    }
}

/// Puts the value in the given table at the given key
pub(crate) async fn table_put_with_key(
    table_name: &str,
    key: &JsValue,
    val: &JsValue,
) -> Result<(), AnyError> {
    // Request a put_with_key() operation on the table
    let table_op = |table: &IdbObjectStore| {
        table
            .put_with_key(val, key)
            .map_err(|e| wrap_jserror("couldn't insert value to table", e))
    };

    // Run the operation
    match access_db(table_name, true, table_op).await {
        Ok(_) => Ok(()),
        Err(e) => Err(anyhow!("Error inserting into {}: {}", table_name, e)),
    }
}

/// Puts the value in the given table and returns the key to it
pub(crate) async fn table_delete(table_name: &str, key: &str) -> Result<(), AnyError> {
    // Request a delete() operation on the table
    let table_op = |table: &IdbObjectStore| {
        table
            .delete(&JsValue::from_str(key))
            .map_err(|e| wrap_jserror("couldn't save value to table", e))
    };

    // Run the operation
    match access_db(table_name, true, table_op).await {
        Ok(_) => {
            tracing::trace!("Successfully deleted {:?}", key);
            Ok(())
        }
        Err(e) => Err(anyhow!("Error deleting from {}: {}", table_name, e)),
    }
}

/// Gets the value in the given table at the given key
pub(crate) async fn table_get(table_name: &str, key: &JsValue) -> Result<JsValue, AnyError> {
    // Request a delete() operation on the table
    let table_op = |table: &IdbObjectStore| {
        table
            .get(key)
            .map_err(|e| wrap_jserror("couldn't get from table", e))
    };

    // Run the operation
    match access_db(table_name, false, table_op).await {
        Ok(val) => {
            tracing::trace!("Successfully got {:?}", val);
            Ok(val)
        }
        Err(e) => Err(anyhow!(
            "Error getting {:?} from {}: {}",
            key,
            table_name,
            e
        )),
    }
}

/// Gets all the keys from the given table
pub(crate) async fn table_get_keys(table_name: &str) -> Result<Vec<JsValue>, AnyError> {
    // Request a delete() operation on the table
    let table_op = |table: &IdbObjectStore| {
        table
            .get_all_keys()
            .map_err(|e| wrap_jserror("couldn't get all keys from table", e))
    };

    // Run the operation
    match access_db(table_name, false, table_op).await {
        Ok(val) => {
            tracing::trace!("Successfully got all keys");

            // Cast the result into a Vec of keys
            Ok(val.dyn_into::<js_sys::Array>().unwrap().to_vec())
        }
        Err(e) => Err(anyhow!("Error getting all keys from {}: {}", table_name, e)),
    }
}

/// Saves the given article to IndexedDB, and returns its title and ID
pub(crate) async fn save_article(article: &CachedArticle) -> Result<QueueEntry, AnyError> {
    // Serialize the article manually. We do this instead of using serde because storing blobs is
    // way faster than storing arrays of integers, which is what serde does.
    let serialized_article = js_sys::Object::new();
    // Set the handle
    js_sys::Reflect::set(
        &serialized_article,
        &JsValue::from_str("id"),
        &JsValue::from_str(&article.id.0),
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

    // Insert the article
    table_put(ARTICLES_TABLE, &serialized_article).await?;

    // Return the article's title and ID
    Ok(article.into())
}

pub(crate) async fn load_article(id: &ArticleId) -> Result<CachedArticle, AnyError> {
    // Request a get() operation on the table
    let serialized_article = table_get(ARTICLES_TABLE, &JsValue::from_str(&id.0)).await?;
    let id = js_sys::Reflect::get(&serialized_article, &JsValue::from_str("id"))
        .map_err(|e| wrap_jserror("couldn't get article id field", e))?
        .as_string()
        .unwrap();
    let js_blob: Blob = js_sys::Reflect::get(&serialized_article, &JsValue::from_str("audio_blob"))
        .unwrap()
        .dyn_into()
        .unwrap();
    let array_buf = JsFuture::from(js_blob.array_buffer()).await.unwrap();
    let audio_blob = js_sys::Uint8Array::new(&array_buf).to_vec();

    Ok(CachedArticle {
        id: ArticleId(id.clone()),
        title: id,
        audio_blob,
    })
}

pub(crate) async fn delete_article(id: &ArticleId) -> Result<(), AnyError> {
    table_delete(ARTICLES_TABLE, &id.0).await
}

/// Saves the article state to IndexedDB
pub(crate) async fn save_article_state(state: &ArticleState) -> Result<(), AnyError> {
    let serialized_state = JsValue::from_serde(&state)?;
    table_put(ARTICLE_STATE_TABLE, &serialized_state).await?;
    Ok(())
}

/// Gets the player state from th IndexedDB
pub(crate) async fn get_article_state(id: &ArticleId) -> Result<ArticleState, AnyError> {
    let key = JsValue::from_str(&id.0);
    table_get(ARTICLE_STATE_TABLE, &key)
        .await
        .and_then(|v| JsValue::into_serde(&v).map_err(Into::into))
}

/// Loads the title and ID of every saved article
pub(crate) async fn load_queue_entries() -> Result<Vec<QueueEntry>, AnyError> {
    let keys = table_get_keys(ARTICLES_TABLE).await?;

    // Convert the table entires into queue entries
    Ok(keys
        .into_iter()
        .map(|v: JsValue| {
            let title = v.as_string().unwrap();
            let id = ArticleId(title.clone());
            QueueEntry { title, id }
        })
        .collect())
}

/// The player state table only holds one value, and that's the current player's state
const PLAYER_STATE_GLOBAL_KEY: f64 = 0.0;

/// Saves the player state to IndexedDB
pub(crate) async fn save_player_state(pos: &PlayerState) -> Result<(), AnyError> {
    let serialized_pos = JsValue::from_serde(&pos)?;
    let key = JsValue::from_f64(PLAYER_STATE_GLOBAL_KEY);
    table_put_with_key(PLAYER_STATE_TABLE, &key, &serialized_pos).await?;
    Ok(())
}

/// Gets the player state from th IndexedDB
pub(crate) async fn get_player_state() -> Result<PlayerState, AnyError> {
    let key = JsValue::from_f64(PLAYER_STATE_GLOBAL_KEY);
    table_get(PLAYER_STATE_TABLE, &key)
        .await
        .and_then(|v| JsValue::into_serde(&v).map_err(Into::into))
}
