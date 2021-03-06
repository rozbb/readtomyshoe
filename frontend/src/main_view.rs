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
                <>
                    { header() }
                    <h3 aria-role="alert" aria-live="assertive" style="color: red">{
                        "Error: cannot access local storage.
                        ReadToMyShoe does not work in private browsing mode in Firefox."
                    }</h3>
                </>
            };
        }

        // Show the main view
        html! {
            <>
                { header() }
                <h2>{ "Player" }</h2>
                    <Player {player_link} />
                <h2>{ "Queue" }</h2>
                    <Queue {queue_link} {player_link} />
                <h2>{ "Library" }</h2>
                    <div id="addArticle">
                        <Link<Route> to={Route::Add} classes="navLink">
                            { "Add Article" }
                        </Link<Route>>
                    </div>
                    <Library {queue_link} />
            </>
        }
    }
}

fn header() -> Html {
    let help_text = html! {
        <>
            <p>{"
                ReadToMyShoe is a website that lets you listen to internet articles and blog posts,
                even when you're offline. ReadToMyShoe is broken up into three sections: the
                "}<strong>{"Library"}</strong>{", the
                "}<strong>{"Queue"}</strong>{", and the
                "}<strong>{"Player"}</strong>{". Here's what each section does:
            "}</p>
            <dl>
                <dt><strong>{ "Library" }</strong></dt>
                <dd>{"
                    The library tells you which articles you have already saved to ReadToMyShoe.
                    To add a new article to your library, click the
                    "}<a class="navLink" href="#addArticle">{"Add Article"}</a>{"
                    button. You cannot play articles directly from the library. Instead, if you
                    want to listen to an article, you first click the \"+\" button beside the
                    article in the library. This adds it to your queue, where it can be played.
                "}</dd>
            </dl>
            <dl>
                <dt><strong>{ "Queue" }</strong></dt>
                <dd>{"
                    The queue stores all the articles that you want to listen to. These articles
                    are fully downloaded to your device, so you can listen to them even without
                    internet connection. To play an article from the queue, press the \"??????\" button
                    next to the article title. The queue will automatically save your place in the
                    article, so you can come back to it later. To delete an article from the queue,
                    press the \"????\" button.
                "}</dd>
            </dl>
            <dl>
                <dt><strong>{ "Player" }</strong></dt>
                <dd>{"
                    The player section contains all the controls you need to adjust playback. You
                    can play and pause, jump backwards and forwards, and set the playback speed.
                    When you load ReadToMyShoe, the player will already be set to the last article
                    you were reading (if any), so all you need to do is press play.
                "}</dd>
            </dl>
            <dl>
                <dt><strong>{ "Bonus features" }</strong></dt>
                <dd>{"There are lots of useful features that this site provides. Here are some."}
                <ul>
                    <li><p><strong>{" Offline mode: " }</strong>{"
                        This site works entirely offline. Go ahead, turn on airplane mode and
                        refresh this page. You should see everything still in your queue. The only
                        thing you can't do is view the library, since the library is in the cloud.
                    "}</p></li>
                    <li><p><strong>{"Add to home screen: " }</strong>{"
                        This website can be added to your homescreen and behave
                        just like a native app. The way to do this varies by device and browser, so
                        you'll have to do some searching to get this set up.
                    "}</p></li>
                    <li><p><strong>{"Control from lockscreen: " }</strong>{"
                        Control from lockscreen: ReadToMyShoe lets you control audio playback from
                        whatever media controls you have on your phone. On the iPhone, for example,
                        you can play, pause, and jump from Control Center, and even from the
                        lockscreen.
                    "}</p></li>
                </ul>
                </dd>
            </dl>
        </>
    };

    html! {
        <header>
            <h1>{ "???? ReadToMyShoe" }</h1>
            <details aria-label="Click to open help">
                <summary class="navLink"><strong>{ "Help" }</strong></summary>
                <div aria-live="polite">{ help_text }</div>
            </details>
        </header>
    }
}
