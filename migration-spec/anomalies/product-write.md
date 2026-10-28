# Product write anomalies

- The legacy update controller has neither `X-Member-Id` nor `@Valid`; it does
  not check seller ownership. Rust preserves that observable authorization gap
  rather than silently changing the contract. JSON shape and category decoding
  are still checked at the HTTP boundary.
- Product creation and update publish recommendation events in Java. Event
  publication is deliberately deferred to the `event-boundaries` migration
  unit; no Kafka or other external broker was added here.
- Idempotency keys, products, deletion markers, and group-purchase usage flags
  are process-local in this repository. The soft-delete branch is covered by
  focused repository/HTTP tests; a later group-purchase unit must feed the
  usage state in the running application.
- The local repository generates RFC 3339 UTC timestamps. It does not provide
  durable database transactions or a live PostgreSQL product integration.
