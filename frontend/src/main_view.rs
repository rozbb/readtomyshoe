use crate::{
    app_view::Route, library_view::Library, player_view::Player, queue_view::Queue,
    WeakComponentLink,
};

use yew::prelude::*;
use yew_router::prelude::*;

pub struct Main {
    /// Indicates whether the app has access to an IndexedDb. If this is false, it's a fatal error
    has_db_access: bool,
}

impl Default for Main {
    fn default() -> Main {
        Main {
            has_db_access: true,
        }
    }
}

#[derive(PartialEq, Properties)]
pub struct Props {
    pub queue_link: WeakComponentLink<Queue>,
    pub player_link: WeakComponentLink<Player>,
}

pub enum Message {
    /// Message indicates that we don't have access to an IndexedDb. This is a fatal error
    DbFailed,
}

impl Component for Main {
    type Message = Message;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        // Kick off a future to test whether or not we have IndexedDB access
        ctx.link().send_future_batch(async move {
            if crate::caching::get_db().await.is_err() {
                vec![Message::DbFailed]
            } else {
                Vec::new()
            }
        });

        Main::default()
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Message::DbFailed => {
                self.has_db_access = false;
            }
        }

        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let queue_link = &ctx.props().queue_link;
        let player_link = &ctx.props().player_link;

        // If we don't have IndexedDB access, don't show anything
        if !self.has_db_access {
            return html! {
                <main>
                    <h1>{ "🥾 ReadToMyShoe" }</h1>
                    <h3 style="color: red">{
                        "Error: cannot access local storage.
                        ReadToMyShoe does not work in private browsing mode in Firefox."
                    }</h3>
                </main>
            };
        }

        // Show the main view
        html! {
            <main>
                <h1>{ "🥾 ReadToMyShoe" }</h1>
                <h2>{ "Library" }</h2>
                    <Link<Route> to={Route::Add} classes="navLink">{ "Add Article" }</Link<Route>>
                    <Library {queue_link} />
                <h2>{ "Queue" }</h2>
                    <Queue {queue_link} {player_link} />
                <h2>{ "Player" }</h2>
                    <Player {player_link} />
            </main>
        }
    }
}
