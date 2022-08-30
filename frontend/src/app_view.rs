use crate::{
    add_view::Add, library_view::Library, main_view::Main, player_view::Player, queue_view::Queue,
    WeakComponentLink,
};

use yew::prelude::*;
use yew_router::prelude::*;

#[derive(Clone, Routable, PartialEq)]
pub enum Route {
    #[at("/")]
    Home,
    #[at("/add")]
    Add,
    #[not_found]
    #[at("/404")]
    NotFound,
}

#[derive(Default)]
pub struct App {
    player_link: WeakComponentLink<Player>,
    queue_link: WeakComponentLink<Queue>,
    library_link: WeakComponentLink<Library>,
}

impl Component for App {
    type Message = ();
    type Properties = ();

    fn create(_ctx: &Context<Self>) -> Self {
        Self::default()
    }

    fn update(&mut self, _ctx: &Context<Self>, _msg: Self::Message) -> bool {
        false
    }

    fn view(&self, _ctx: &Context<Self>) -> Html {
        let player_link_copy = self.player_link.clone();
        let queue_link_copy = self.queue_link.clone();
        let library_link_copy = self.library_link.clone();

        let switch = move |routes: &Route| {
            let player_link = player_link_copy.clone();
            let queue_link = queue_link_copy.clone();
            let library_link = library_link_copy.clone();

            match routes {
                Route::Home => html! {
                    <Main {player_link} {queue_link} {library_link} />
                },
                Route::Add => html! {
                    <Add />
                },
                Route::NotFound => html! { <h1>{ "404" }</h1> },
            }
        };

        html! {
        <BrowserRouter>
            <Switch<Route> render={Switch::render(switch)} />
        </BrowserRouter>
        }
    }
}
