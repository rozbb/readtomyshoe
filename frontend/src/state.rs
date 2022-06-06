use gloo_net::http::Request;
use gloo_storage::LocalStorage;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;
use yew_router::prelude::*;

#[derive(Default, Serialize, Deserialize)]
struct ReadToMyShoeState {
    /// Keys to CachedArticles
    playqueue: Vec<String>,
    /// The index of the article currently playing
    cur_article: usize,
}

const GLOBAL_STATE_KEY: &str = "readtomyshoe-state";

struct LocalPlayqueue;

impl LocalPlayqueue {
    fn init() {
        LocalStorage::set(GLOBAL_STATE_KEY, ReadToMyShoeState::default()).unwrap();
    }

    fn add(article: CachedArticle) {
        // Save the cached article in local storage
        LocalStorage.set(article.title, article).unwrap();

        // Update the state
        let state = LocalStorage::get(GLOBAL_STATE_KEY).unwrap();
        state.playqueue.push(article.title);
        LocalStorage::set(GLOBAL_STATE_KEY, state).unwrap();
    }

    fn remove(idx: usize) {
        let state = LocalStorage::get(GLOBAL_STATE_KEY).unwrap();
        state.playqueue.remove(idx);
        LocalStorage::set(GLOBAL_STATE_KEY, state).unwrap();
    }

    fn get(idx: usize) -> Option<CachedArticle> {
        let state = LocalStorage::get(GLOBAL_STATE_KEY).unwrap();
        state.playqueue.get(idx)
    }

    fn get_cur() -> Option<CachedArticle> {
        let state = LocalStorage::get(GLOBAL_STATE_KEY).unwrap();
        let idx = state.cur_pos;
        state.playqueue.get(idx)
    }

    fn inc() -> Result<(), ()> {
        let mut state = LocalStorage::get(GLOBAL_STATE_KEY).unwrap();
        let idx = &mut state.cur_pos;

        if *idx == state.playqueue.len() - 1 {
            Err(())
        } else {
            *idx += 1;
            LocalStorage::set(GLOBAL_STATE_KEY, state).unwrap();
            Ok(())
        }
    }
}
