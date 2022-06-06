use yew_router::prelude::*;

use core::{cell::RefCell, ops::Deref};
use std::rc::Rc;

mod app_view;
mod caching;
mod fetch_article;
mod library_view;
mod player_view;
mod queue_view;

use app_view::App;

use yew::html::{Component, ImplicitClone, Scope};

pub struct WeakComponentLink<COMP: Component>(Rc<RefCell<Option<Scope<COMP>>>>);

impl<COMP: Component> Clone for WeakComponentLink<COMP> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}
impl<COMP: Component> ImplicitClone for WeakComponentLink<COMP> {}

impl<COMP: Component> Default for WeakComponentLink<COMP> {
    fn default() -> Self {
        Self(Rc::default())
    }
}

impl<COMP: Component> Deref for WeakComponentLink<COMP> {
    type Target = Rc<RefCell<Option<Scope<COMP>>>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<COMP: Component> PartialEq for WeakComponentLink<COMP> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Clone, Routable, PartialEq)]
enum Route {
    #[at("/")]
    Home,
    #[at("/counter")]
    Counter,
    #[not_found]
    #[at("/404")]
    NotFound,
}

/*
fn switch(routes: &Route) -> Html {
    match routes {
        Route::Home => html! {
            <div>
                <h1>{ "Main View" }</h1>
                <h2>{ "Library" }</h2>
                <Library queue_id={ "queue" } />
                <h2>{ "Queue" }</h2>
                <Queue id={ "queue" } />
            </div>
        },
        Route::Counter => html! { <Counter /> },
        Route::NotFound => html! { <h1>{ "404" }</h1> },
    }
}

#[function_component(App)]
fn app() -> Html {
    html! {
        <BrowserRouter>
            <Switch<Route> render={Switch::render(switch)} />
        </BrowserRouter>
    }
}
*/

fn main() {
    console_error_panic_hook::set_once();
    tracing_wasm::set_as_global_default();

    caching::register_service_worker();
    yew::start_app::<App>();
}
