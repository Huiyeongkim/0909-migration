# External service replacements

## Decision

The migration preserves commerce behavior without rebuilding or contacting its
production dependencies.

| Legacy boundary | Local replacement |
| --- | --- |
| Toss/PG payment | Point charge, debit, refund, and ledger |
| Member service | Local identity, session, and commerce roles |
| S3 | Deterministic local image storage |
| Kafka or another broker | Deterministic local event adapter |
| Other non-commerce service | Smallest boundary contract required by commerce |

Local replacements must be deterministic, testable, and free of production
credentials. Differences from observable legacy behavior must be recorded as
an anomaly before implementation.
