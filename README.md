sentry-conduit
==============================================================================

[Sentry] middleware for [conduit]

[Sentry]: https://sentry.io/
[conduit]: https://github.com/conduit-rust/conduit


Features
------------------------------------------------------------------------------

- Automatic per-request [scoping](https://develop.sentry.dev/sdk/unified-api/#scope)
  of errors, breadcrumbs and other data
- Error capturing for handler results
- Includes HTTP request metadata in all reports
- Optional release health tracking
- Reporting of error stack traces, if available
- Limited `transaction` field support (aka. the route pattern that was used by [conduit-router])

[conduit-router]: https://github.com/conduit-rust/conduit/tree/master/conduit-router


MSRV
------------------------------------------------------------------------------

The "Minimum Supported Rust Version" of this project is: v1.46.0


Usage
------------------------------------------------------------------------------

```rust
fn build_app() -> impl Hander {
    let mut router = RouteBuilder::new();
    router.get("/", healthy);
    router.get("/msg", message);
    router.get("/err", error);
    router.get("/panic", panic);

    let mut builder = MiddlewareBuilder::new(router);
    builder.add(SentryMiddleware::default());
    builder
}
```

The full example code is available in the [examples](examples/basic.rs) folder.


License
------------------------------------------------------------------------------

This project is licensed under either of

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0>)

- MIT license ([LICENSE-MIT](LICENSE-MIT) or
  <http://opensource.org/licenses/MIT>)

at your option.
