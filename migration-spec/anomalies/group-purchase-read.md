# Group-purchase read anomalies

- The legacy controller specifies no default order. The Rust repository likewise preserves insertion/storage order when no `sort` query is supplied instead of inventing an order.
- Rust accepts one allowlisted Spring-style sort (`property,direction`). Unknown properties fall back to unsorted results; the legacy tests do not define their error behavior.
- The legacy service performs one product repository lookup per group purchase. Rust keeps the same repository boundaries and observable missing-product outcome; durable group-purchase persistence remains outside this read slice.
- Negative `page` values fail query decoding as HTTP 400 in Rust. Spring's negative pageable error is framework-generated and is not asserted by the legacy tests.
