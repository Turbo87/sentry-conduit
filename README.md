sentry-conduit
==============================================================================

[Sentry] middleware for [conduit]

[Sentry]: https://sentry.io/
[conduit]: https://github.com/conduit-rust/conduit


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
    builder.around(SentryMiddleware::default());
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
