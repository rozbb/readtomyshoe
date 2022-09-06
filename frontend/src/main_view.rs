use crate::{library_view::Library, player_view::Player, queue_view::Queue, WeakComponentLink};

use yew::prelude::*;

// TODO Fixme: This path is only valid in production mode
const LOGO_PATH: &str = "/assets/rtms-color-180x180.png";

pub(crate) struct Main {
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
pub(crate) struct Props {
    pub player_link: WeakComponentLink<Player>,
    pub queue_link: WeakComponentLink<Queue>,
    pub library_link: WeakComponentLink<Library>,
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
        let player_link = &ctx.props().player_link;
        let queue_link = &ctx.props().queue_link;
        let library_link = &ctx.props().library_link;

        // If we don't have IndexedDB access, don't show anything
        if !self.has_db_access {
            return html! {
                <>
                    { header() }
                    <h3 role="alert" style="color: red">{
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
                <Player {player_link} {queue_link}  />
                <Queue {player_link} {queue_link} {library_link} />
                <Library {queue_link} {library_link} />
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
                    "}<a href="#addArticle">{"Add Article"}</a>{"
                    button. You cannot play articles directly from the library. Instead, if you
                    want to listen to an article, you first click the \"+\" button beside the
                    article in the library. This adds it to your queue, where it can be played.
                "}</dd>
                <dt><strong>{ "Queue" }</strong></dt>
                <dd>{"
                    The queue stores all the articles that you want to listen to. These articles
                    are fully downloaded to your device, so you can listen to them even without
                    internet connection. To play an article from the queue, press the \"▶️\" button
                    next to the article title. The queue will automatically save your place in the
                    article, so you can come back to it later. To delete an article from the queue,
                    press the \"🗑\" button.
                "}</dd>
                <dt><strong>{ "Player" }</strong></dt>
                <dd>{"
                    The player section contains all the controls you need to adjust playback. You
                    can play and pause, jump backwards and forwards, and set the playback speed.
                    When you load ReadToMyShoe, the player will already be set to the last article
                    you were reading (if any), so all you need to do is press play.
                "}</dd>
                <dt><strong>{ "Bonus features" }</strong></dt>
                <dd>{"There are lots of useful features that this site provides. Here are some."}
                    <ul>
                        <li><p><strong>{" Offline mode: " }</strong>{"
                            This site works entirely offline. Go ahead, turn on airplane mode and
                            refresh this page. You should see everything still in your queue. The
                            only thing you can't do is view the library, since the library is in
                            the cloud.
                        "}</p></li>
                        <li><p><strong>{"Add to home screen: " }</strong>{"
                            This website can be added to your homescreen and behave just like a
                            native app. The way to do this varies by device and browser, so you'll
                            have to do some searching to get this set up.
                        "}</p></li>
                        <li><p><strong>{"Control from lockscreen: " }</strong>{"
                            ReadToMyShoe lets you control audio playback from whatever media
                            controls you have on your device. On the iPhone, for example, you can
                            play, pause, and jump from Control Center, and even from the lockscreen.
                        "}</p></li>
                    </ul>
                </dd>
            </dl>
        </>
    };

    html! {
        <header>
            <img class="headerLogo" src={LOGO_PATH}
             alt="ReadToMyShoe logo: A sneaker wearing a headset with a microphone" />
            <h1>{ "ReadToMyShoe" }</h1>
            <nav>
                <a href="https://github.com/rozbb/readtomyshoe">{"About"}</a>
                <details>
                    <summary><span id="helpLink">{ "Help" }</span></summary>
                    <div aria-live="polite">{ help_text }</div>
                </details>
            </nav>
            <hr />
        </header>
    }
}
