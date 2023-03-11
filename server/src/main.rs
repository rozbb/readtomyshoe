mod add_article;
mod error;
mod lang;
mod list_articles;
mod tts;
mod util;

use std::{
    future::ready,
    net::{IpAddr, Ipv6Addr, SocketAddr},
    num::NonZeroU32,
    path::PathBuf,
    str::FromStr,
};

use axum::{
    body::Body,
    http::{HeaderValue, Request, StatusCode},
    response::Response,
    routing::{get, get_service},
    Router,
};
use clap::Parser;
use tower::ServiceBuilder;
use tower_http::{
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

#[derive(Parser, Debug)]
#[clap(
    name = "readtomyshoe-server",
    about = "The primary backend for Readtomyshoe"
)]
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
    #[clap(short = 'p', long = "port", default_value = "9382")]
    port: u16,

    /// The directory where static files are to be found
    #[clap(long = "static-dir", default_value = "../dist")]
    static_dir: String,

    /// The directory where the audio blobs are stored
    #[clap(long = "audio-blob-dir", default_value = "audio_blobs")]
    audio_blob_dir: String,

    /// The limit on the number of article characters (bytes, really) the server will process per
    /// minute. The default is 5M because that's Google Cloud's limit. Use large values with
    /// caution: a malicious user can rack up your Google Cloud costs.
    #[clap(long = "max-chars-per-min", default_value = "5000000")]
    max_chars_per_min: NonZeroU32,
}

#[tokio::main]
async fn main() {
    let opt = Opt::parse();

    // Check up front that the Google cloud API key was set
    tts::get_api_key().unwrap();

    // Setup logging & RUST_LOG from args
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", format!("{},hyper=info,mio=info", opt.log_level))
    }

    tracing_subscriber::fmt::init();

    // A generic error handler that just returns 500
    let ret_500 = |_| ready(StatusCode::INTERNAL_SERVER_ERROR);

    // Make a service that returns the static assets from /assets, and serves index.html
    // everywhere else. Also try to serve gzipped assets when the .gz file exists
    let static_dir: PathBuf = opt.static_dir.as_str().into();
    let asset_service = get_service(ServeDir::new(&opt.static_dir).precompressed_gzip());
    let index_service =
        get_service(ServeFile::new(static_dir.join(&opt.index_file)).precompressed_gzip());
    let asset_router = Router::new()
        .nest("/assets", asset_service.handle_error(ret_500.clone()))
        .layer(ServiceBuilder::new().map_response(add_asset_headers))
        .fallback(index_service.handle_error(ret_500.clone()));

    // Make a service that just returns files from /audio_blobs
    let audio_blob_service =
        get_service(ServeDir::new("audio_blobs")).handle_error(ret_500.clone());
    let app = asset_router.nest("/api/audio-blobs", audio_blob_service);

    // Set up /api/
    let app = list_articles::setup(app, &opt.audio_blob_dir);
    let app = add_article::setup(app, opt.max_chars_per_min, &opt.audio_blob_dir);

    // Make a /healthz endpoint for Docker health checks
    let app = app.route("/healthz", get(|| async { "ok" }));

    // Tracing for the entire app
    let app = app.layer(
        TraceLayer::new_for_http().make_span_with(|req: &Request<Body>| {
            // Log the IP given by the reverse proxy
            tracing::debug_span!("http-request", client_ip = ?req.headers().get("X-Forwarded-For"))
        }),
    );

    // Set up the server
    let sock_addr = SocketAddr::from((
        IpAddr::from_str(opt.addr.as_str()).unwrap_or(IpAddr::V6(Ipv6Addr::LOCALHOST)),
        opt.port,
    ));
    tracing::info!("Listening on http://{}", sock_addr);
    axum::Server::bind(&sock_addr)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .expect("Unable to start server");
}

/// Adds headers for ReadToMyShoe assets. Assets are cached by the service worker, and not the HTTP
/// cache
fn add_asset_headers(mut resp: Response) -> Response {
    let headers = resp.headers_mut();

    // Add a Service-Worker-Allowed header to the static assets. This allows a service worker from
    // /assets/ to cache items from the root directory
    headers.insert("Service-Worker-Allowed", HeaderValue::from_static("/"));

    // Do not use the HTTP cache. The client's service worker does one level of caching
    headers.insert(
        "Cache-control",
        HeaderValue::from_static("public, no-cache"),
    );
    resp
}
