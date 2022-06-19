use crate::{library_view::Library, player_view::Player, queue_view::Queue, WeakComponentLink};

use yew::prelude::*;

pub struct Main;

#[derive(PartialEq, Properties)]
pub struct Props {
    pub queue_link: WeakComponentLink<Queue>,
    pub player_link: WeakComponentLink<Player>,
}

impl Component for Main {
    type Message = ();
    type Properties = Props;

    fn create(_ctx: &Context<Self>) -> Self {
        Main
    }

    fn update(&mut self, _ctx: &Context<Self>, _msg: Self::Message) -> bool {
        false
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let queue_link = &ctx.props().queue_link;
        let player_link = &ctx.props().player_link;

        html! {
            <div class="main">
                <h1>{ "Main View" }</h1>
                <h2>{ "Library" }</h2>
                <a href="/add" style="font-weight: bold">{ "Add Article" }</a>
                <Library {queue_link} />
                <h2>{ "Queue" }</h2>
                <Queue {queue_link} {player_link} />
                <h2>{ "Player" }</h2>
                <Player {player_link} />
            </div>
        }
    }
}
