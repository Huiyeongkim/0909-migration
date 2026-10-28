# seller-balance anomalies

- Legacy balance creation is not idempotent despite the unique member constraint. Rust preserves the first create and maps a duplicate to the generic internal-error response.
- History lookup does not require a balance row. It filters the history table by the authenticated member and may return an empty successful page.
- Legacy declares no default history ordering. Rust likewise applies no order without a sort query and interpolates only allowlisted columns for explicit sorting.
- The history schema preserves nullable payout and order-settlement references.
- Focused tests use the repository's test adapter. No live PostgreSQL integration test is required by this unit.
