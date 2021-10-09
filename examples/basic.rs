use conduit::{header, Body, Handler, RequestExt, Response, ResponseResult};
use conduit_hyper::Server;
use conduit_middleware::MiddlewareBuilder;
use conduit_router::RouteBuilder;
use sentry::Level;
use sentry_conduit::SentryMiddleware;
use std::io;
use tracing::info;
use tracing_subscriber::{filter, prelude::*};

#[tokio::main]
async fn main() {
    let _guard = sentry::init((
        std::env::var("SENTRY_DSN").unwrap(),
        sentry::ClientOptions {
            auto_session_tracking: true,
            release: sentry::release_name!(),
            session_mode: sentry::SessionMode::Request,
            ..Default::default()
        },
    ));

    std::env::set_var("RUST_BACKTRACE", "1");

    let log_filter = std::env::var("RUST_LOG")
        .unwrap_or_default()
        .parse::<filter::Targets>()
        .expect("Invalid RUST_LOG value");

    let sentry_filter = filter::Targets::new().with_default(tracing::Level::INFO);

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_filter(log_filter))
        .with(sentry::integrations::tracing::layer().with_filter(sentry_filter))
        .init();

    let addr = "127.0.0.1:3001";
    println!("Starting server on http://{}", addr);

    let addr = addr.parse().unwrap();
    let app = build_conduit_handler();
    Server::serve(&addr, app).await;
}

fn build_conduit_handler() -> impl Handler {
    let mut router = RouteBuilder::new();
    router.get("/", healthy);
    router.get("/err", errors);
    router.get("/msg", captures_message);
    router.get("/panic", panic);

    let mut builder = MiddlewareBuilder::new(router);
    builder.around(SentryMiddleware::default());
    builder
}

fn basic_response(body: &'static str) -> Result<Response<Body>, io::Error> {
    let body = body.as_bytes();

    let response = Response::builder()
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header(header::CONTENT_LENGTH, body.len())
        .body(Body::from_static(body))
        .unwrap();

    Ok(response)
}

fn healthy(_req: &mut dyn RequestExt) -> ResponseResult<io::Error> {
    sentry::add_breadcrumb(sentry::Breadcrumb {
        message: Some("regular breadcrumb".into()),
        ..Default::default()
    });

    basic_response("All good")
}

fn errors(_req: &mut dyn RequestExt) -> ResponseResult<io::Error> {
    sentry::add_breadcrumb(sentry::Breadcrumb {
        message: Some("error breadcrumb".into()),
        ..Default::default()
    });

    Err(io::Error::new(
        io::ErrorKind::Other,
        "An error happens here",
    ))
}

fn captures_message(_req: &mut dyn RequestExt) -> ResponseResult<io::Error> {
    sentry::add_breadcrumb(sentry::Breadcrumb {
        message: Some("message breadcrumb".into()),
        ..Default::default()
    });

    let some_number = 42;
    info!(some_number, "tracing breadcrumb");

    sentry::capture_message("Something is not well", Level::Warning);
    basic_response("Hello World")
}

fn panic(_req: &mut dyn RequestExt) -> ResponseResult<io::Error> {
    sentry::add_breadcrumb(sentry::Breadcrumb {
        message: Some("panic breadcrumb".into()),
        ..Default::default()
    });
    panic!("message");
}
