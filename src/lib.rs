use conduit::{Handler, HandlerResult, Host, RequestExt, Scheme};
use conduit_middleware::AroundMiddleware;
use sentry_core::protocol::{ClientSdkPackage, Event, Request, SessionStatus};
use std::borrow::Cow;

pub struct SentryMiddleware {
    handler: Option<Box<dyn Handler>>,
}

impl Default for SentryMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

impl SentryMiddleware {
    pub fn new() -> SentryMiddleware {
        SentryMiddleware { handler: None }
    }
}

impl AroundMiddleware for SentryMiddleware {
    fn with_handler(&mut self, handler: Box<dyn Handler>) {
        self.handler = Some(handler)
    }
}

impl Handler for SentryMiddleware {
    fn call(&self, req: &mut dyn RequestExt) -> HandlerResult {
        sentry_core::with_scope(
            |_scope| {},
            || {
                let (with_pii, track_sessions) = sentry_core::Hub::with_active(|hub| {
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

                if track_sessions {
                    sentry_core::start_session();
                }

                let sentry_req = sentry_request_from_http(req, with_pii);
                sentry_core::configure_scope(|scope| {
                    scope.add_event_processor(Box::new(move |event| {
                        Some(process_event(event, &sentry_req))
                    }))
                });

                let result = self.handler.as_ref().unwrap().call(req);

                #[cfg(feature = "router")]
                {
                    // unfortunately, `RoutePattern` is only available *after* the call above
                    // so we can't add the `transaction` field to any captures that happen
                    // inside of the handlers.
                    sentry_core::configure_scope(|scope| {
                        use conduit_router::RoutePattern;

                        let transaction = req
                            .extensions()
                            .find::<RoutePattern>()
                            .map(|pattern| pattern.pattern());

                        scope.set_transaction(transaction);
                    });
                }

                if let Err(error) = &result {
                    sentry_core::capture_error(error.as_ref());
                }

                if track_sessions {
                    let status = match &result {
                        Ok(_) => SessionStatus::Exited,
                        Err(_) => SessionStatus::Abnormal,
                    };
                    sentry_core::end_session_with_status(status);
                }

                result
            },
        )
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
