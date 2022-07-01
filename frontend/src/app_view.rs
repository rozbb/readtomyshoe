use crate::{
    add_view::Add, main_view::Main, player_view::Player, queue_view::Queue, WeakComponentLink,
};

use yew::prelude::*;
use yew_router::prelude::*;

#[derive(Clone, Routable, PartialEq)]
enum Route {
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
    queue_link: WeakComponentLink<Queue>,
    player_link: WeakComponentLink<Player>,
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
        let queue_link_copy = self.queue_link.clone();
        let player_link_copy = self.player_link.clone();

        let switch = move |routes: &Route| {
            let queue_link = queue_link_copy.clone();
            let player_link = player_link_copy.clone();

            match routes {
                Route::Home => html! {
                    <Main {queue_link} {player_link} />
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
