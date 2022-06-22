use conduit::{Host, RequestExt, Scheme, StatusCode};
use conduit_middleware::{AfterResult, BeforeResult, Middleware};
use sentry_core::protocol::{ClientSdkPackage, Event, Request, SessionStatus, SpanStatus};
use sentry_core::{Hub, ScopeGuard, TransactionOrSpan};
use std::borrow::Cow;

pub struct SentryMiddleware {
    start_transactions: bool,
    track_sessions: bool,
    with_pii: bool,
}

impl Default for SentryMiddleware {
    fn default() -> Self {
        // Read `send_default_pii` and `auto_session_tracking` options from
        // Sentry configuration by default
        let (with_pii, track_sessions) = Hub::with_active(|hub| {
            let client = hub.client();

            let with_pii = client
                .as_ref()
                .map_or(false, |client| client.options().send_default_pii);

            let track_sessions = client.as_ref().map_or(false, |client| {
                let options = client.options();
                options.auto_session_tracking
                    && options.session_mode == sentry_core::SessionMode::Request
            });

            (with_pii, track_sessions)
        });

        SentryMiddleware {
            start_transactions: false,
            track_sessions,
            with_pii,
        }
    }
}

impl SentryMiddleware {
    pub fn new() -> SentryMiddleware {
        Default::default()
    }

    pub fn with_transactions() -> SentryMiddleware {
        SentryMiddleware {
            start_transactions: true,
            ..SentryMiddleware::default()
        }
    }
}

impl Middleware for SentryMiddleware {
    fn before(&self, req: &mut dyn RequestExt) -> BeforeResult {
        // Push a `Scope` to the stack so that all further `configure_scope()`
        // calls are scoped to this specific request.
        let scope = Hub::with_active(|hub| hub.push_scope());

        // Start a `Session`, if session tracking is enabled
        if self.track_sessions {
            sentry_core::start_session();
        }

        // Extract HTTP request information from `req`
        let sentry_req = sentry_request_from_http(req, self.with_pii);

        if self.start_transactions {
            // Request path will be taken as the default transaction name
            let name = req.path();

            // Create a HTTP headers iterator that Sentry can ingest
            let headers = req.headers().iter().flat_map(|(header, value)| {
                value.to_str().ok().map(|value| (header.as_str(), value))
            });

            // Create a new transaction context
            let ctx = sentry_core::TransactionContext::continue_from_headers(
                name,
                "http.server",
                headers,
            );

            // ... and turn it into a `Transaction`
            let transaction = sentry_core::start_transaction(ctx);

            // Attach HTTP request data to the transaction
            transaction.set_request(sentry_req.clone());

            // Configure Sentry to use the `Transaction` for this request
            sentry_core::configure_scope(|scope| scope.set_span(Some(transaction.into())));
        }

        // .Configure Sentry to use the HTTP request data for issue reports
        sentry_core::configure_scope(|scope| {
            scope.add_event_processor(Box::new(move |event| {
                Some(process_event(event, &sentry_req))
            }));
        });

        // Save the `ScopeGuard` in the request to ensure that it's not dropped yet
        req.mut_extensions().insert(scope);

        Ok(())
    }

    fn after(&self, req: &mut dyn RequestExt, result: AfterResult) -> AfterResult {
        if let Some(scope) = req.mut_extensions().remove::<ScopeGuard>() {
            #[cfg(feature = "router")]
            {
                sentry_core::configure_scope(|scope| {
                    // unfortunately, `RoutePattern` is only available in the `after` handler
                    // so we can't add the `transaction` field to any captures that happen
                    // before this is called.
                    use conduit_router::RoutePattern;

                    let transaction = req
                        .extensions()
                        .get::<RoutePattern>()
                        .map(|pattern| pattern.pattern());

                    scope.set_transaction(transaction);
                });
            }

            // If a `Transaction` was started for this request ...
            if let Some(TransactionOrSpan::Transaction(transaction)) =
                sentry_core::configure_scope(|scope| scope.get_span())
            {
                // Set the HTTP response status code on the transaction
                // if it has not been set yet.
                if transaction.get_status().is_none() {
                    let status = result
                        .as_ref()
                        .map(|res| map_status(res.status()))
                        .unwrap_or(SpanStatus::UnknownError);
                    transaction.set_status(status);
                }

                // Finish the `Transaction` (aka. send it to Sentry)
                transaction.finish();
            }

            // Capture `Err` results as errors
            if let Err(error) = &result {
                sentry_core::capture_error(error.as_ref());
            }

            // End the `Session`, if session tracking is enabled
            if self.track_sessions {
                let status = match &result {
                    Ok(_) => SessionStatus::Exited,
                    Err(_) => SessionStatus::Abnormal,
                };
                sentry_core::end_session_with_status(status);
            }

            // Explicitly drop the `Scope` (technically unnecessary)
            drop(scope);
        }

        result
    }
}

/// Build a Sentry request struct from the HTTP request
fn sentry_request_from_http(request: &dyn RequestExt, with_pii: bool) -> Request {
    let method = Some(request.method().to_string());

    let scheme = match request.scheme() {
        Scheme::Http => "http",
        Scheme::Https => "https",
    };

    let host = match request.host() {
        Host::Name(name) => Cow::from(name),
        Host::Socket(addr) => Cow::from(addr.to_string()),
    };

    let path = request.path();

    let mut url = format!("{}://{}{}", scheme, host, path);

    if let Some(query_string) = request.query_string() {
        url += "?";
        url += query_string;
    }

    let headers = request
        .headers()
        .iter()
        .filter(|(_name, value)| !value.is_sensitive())
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or_default().to_string()))
        .collect();

    let mut sentry_req = Request {
        url: url.parse().ok(),
        method,
        headers,
        ..Default::default()
    };

    // If PII is enabled, include the remote address
    if with_pii {
        let remote_addr = request.remote_addr().to_string();
        sentry_req.env.insert("REMOTE_ADDR".into(), remote_addr);
    };

    sentry_req
}

/// Turn `http::StatusCode` into a type that Sentry understands.
fn map_status(status: StatusCode) -> SpanStatus {
    match status {
        StatusCode::UNAUTHORIZED => SpanStatus::Unauthenticated,
        StatusCode::FORBIDDEN => SpanStatus::PermissionDenied,
        StatusCode::NOT_FOUND => SpanStatus::NotFound,
        StatusCode::TOO_MANY_REQUESTS => SpanStatus::ResourceExhausted,
        StatusCode::CONFLICT => SpanStatus::AlreadyExists,
        StatusCode::NOT_IMPLEMENTED => SpanStatus::Unimplemented,
        StatusCode::SERVICE_UNAVAILABLE => SpanStatus::Unavailable,
        status if status.is_informational() => SpanStatus::Ok,
        status if status.is_success() => SpanStatus::Ok,
        status if status.is_redirection() => SpanStatus::Ok,
        status if status.is_client_error() => SpanStatus::InvalidArgument,
        status if status.is_server_error() => SpanStatus::InternalError,
        _ => SpanStatus::UnknownError,
    }
}

/// Add request data to a Sentry event
fn process_event(mut event: Event<'static>, request: &Request) -> Event<'static> {
    // Request
    if event.request.is_none() {
        event.request = Some(request.clone());
    }

    // SDK
    if let Some(sdk) = event.sdk.take() {
        let mut sdk = sdk.into_owned();
        sdk.packages.push(ClientSdkPackage {
            name: "sentry-conduit".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        });
        event.sdk = Some(Cow::Owned(sdk));
    }

    event
}
