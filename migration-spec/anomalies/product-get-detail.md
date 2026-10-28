# Product detail lookup anomalies

- The legacy service test covers found and missing products, but the inspected
  controller test has no dedicated detail-lookup case. Rust tests therefore
  pin both HTTP outcomes at the handler boundary.
- Product timestamps were created as timestamp-without-time-zone in V1 and
  converted to `TIMESTAMPTZ` in V4_3. This slice keeps their RFC 3339 JSON form
  as strings and does not reinterpret the offset.
- The current local repository is intentionally process-local and starts
  empty. Durable storage and product creation belong to later migration units;
  the repository trait keeps that replacement at the persistence boundary.
