use conduit::{Handler, RequestExt};
use conduit_middleware::{AfterResult, AroundMiddleware};

pub struct SentryMiddleware {
    handler: Option<Box<dyn Handler>>,
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
    fn call(&self, req: &mut dyn RequestExt) -> AfterResult {
        sentry_core::with_scope(
            |_scope| {},
            || {
                let result = self.handler.as_ref().unwrap().call(req);
                if let Err(error) = &result {
                    sentry_core::capture_error(error.as_ref());
                }
                result
            },
        )
    }
}
