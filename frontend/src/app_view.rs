use crate::{library_view::Library, player_view::Player, queue_view::Queue, WeakComponentLink};

use yew::prelude::*;

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
        let queue_link = &self.queue_link;
        let player_link = &self.player_link;

        html! {
            <div class="main">
                <h1>{ "Main View" }</h1>
                <h2>{ "Library" }</h2>
                <Library {queue_link} />
                <h2>{ "Queue" }</h2>
                <Queue {queue_link} {player_link} />
                <h2>{ "Player" }</h2>
                <Player {player_link} />
            </div>
        }
    }
}
