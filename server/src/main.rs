mod counter;
mod list_articles;

use std::future::ready;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::str::FromStr;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{HeaderValue, Request, StatusCode};
use axum::routing::{get, get_service};
use axum::Router;
use axum_extra::routing::SpaRouter;
use clap::Parser;
use tower::ServiceBuilder;
use tower_http::{
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

#[derive(Parser, Debug)]
#[clap(name = "{{project-name}}-server", about = "A Rust web server.")]
struct Opt {
    /// The log level
    #[clap(short = 'l', long = "log", default_value = "debug")]
    log_level: String,

    /// The default file to serve, aka index.html
    #[clap(short = 'i', long = "index", default_value = "index.html")]
    index_file: String,

    /// The listen addr
    #[clap(short = 'a', long = "addr", default_value = "::1")]
    addr: String,

    /// The listen port
    #[clap(short = 'p', long = "port", default_value = "8080")]
    port: u16,

    /// The directory where static files are to be found
    #[clap(long = "static-dir", default_value = "../dist")]
    static_dir: String,

    /// The directory where the audio blobs are stored
    #[clap(long = "audio-blob-dir", default_value = "audio_blobs")]
    audio_blob_dir: String,
}

#[tokio::main]
async fn main() {
    let opt = Opt::parse();

    // Setup logging & RUST_LOG from args
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", format!("{},hyper=info,mio=info", opt.log_level))
    }

    tracing_subscriber::fmt::init();

    let audio_blob_service = get_service(ServeDir::new("audio_blobs"))
        .handle_error(|_| ready(StatusCode::INTERNAL_SERVER_ERROR));

    let app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .nest("/api/audio-blobs", audio_blob_service);
    //.merge(SpaRouter::new("/assets", &opt.static_dir).index_file(&opt.index_file))

    let app = counter::setup(app);
    let app = list_articles::setup(app, &opt.audio_blob_dir);

    let tracing_layer = TraceLayer::new_for_http();
    let app = app.layer(ServiceBuilder::new().layer(tracing_layer));

    let sock_addr = SocketAddr::from((
        IpAddr::from_str(opt.addr.as_str()).unwrap_or(IpAddr::V6(Ipv6Addr::LOCALHOST)),
        opt.port,
    ));

    tracing::info!("listening on http://{}", sock_addr);

    axum::Server::bind(&sock_addr)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .expect("Unable to start server");
}
