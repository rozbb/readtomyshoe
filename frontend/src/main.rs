use core::{cell::RefCell, ops::Deref};
use std::rc::Rc;

mod add_view;
mod app_view;
mod caching;
mod library_view;
mod main_view;
mod player_view;
mod queue_view;

use app_view::App;

use yew::html::{Component, ImplicitClone, Scope};

pub struct WeakComponentLink<C: Component>(Rc<RefCell<Option<Scope<C>>>>);

impl<C: Component> Clone for WeakComponentLink<C> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}
impl<C: Component> ImplicitClone for WeakComponentLink<C> {}

impl<C: Component> Default for WeakComponentLink<C> {
    fn default() -> Self {
        Self(Rc::default())
    }
}

impl<C: Component> Deref for WeakComponentLink<C> {
    type Target = Rc<RefCell<Option<Scope<C>>>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<C: Component> PartialEq for WeakComponentLink<C> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

fn main() {
    console_error_panic_hook::set_once();
    tracing_wasm::set_as_global_default();

    caching::register_service_worker();

    yew::start_app::<App>();
}
