# Transactional outbox

## Decision

Commerce state changes and their publishable local events are written in the same PostgreSQL transaction. The outbox replaces direct best-effort logging at the transaction boundary; event delivery still uses the approved local structured-log publisher rather than Kafka.

## Delivery

- Pending rows are selected in occurrence order.
- Each row is claimed with `FOR UPDATE SKIP LOCKED` so concurrent processors do not publish the same locked row.
- Success records `published_at` and increments `attempts`.
- Failure records `last_error`, increments `attempts`, and leaves `published_at` empty for the next run.
- `POST /internal/outbox/process` runs a bounded batch of 100 events.

Delivery is at-least-once: a process failure after publishing but before marking the row may publish the same event again. Consumers must use `eventId` for deduplication.

## Idempotency

`(aggregate_type, aggregate_id, event_type)` is unique. Retrying the same state transition cannot create another logical event row.

## Verification

The PostgreSQL test verifies rollback with business state, duplicate enqueue suppression, failed delivery retention, and successful retry. The PostgreSQL-enabled full suite passes with 89 tests.
