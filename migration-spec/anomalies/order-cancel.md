# Order cancellation anomalies

## Seller rejection leaves the legacy order canceled

The legacy request marks the order `CANCELLED` before seller review. Rejection
changes only the cancellation row to `REJECTED`, leaving a paid order canceled.
The approved target restores `PAYMENT_COMPLETED` on rejection.

## Policy boundaries are missing and refunds can be negative

Legacy predicates exclude exactly 48 hours and exactly two weeks. The return
policy also always subtracts 6,000 won after the 20 percent fee, which can
produce a negative refund for low-value orders. The approved target closes the
boundaries and caps shipping at the remaining amount.

## Idempotency is checked before ownership

The legacy service returns early when an order already has a cancellation,
before loading the order or validating its buyer. Its idempotency key is also
globally unique and insertion uses check-then-insert. The target validates the
non-deleted order and owner first, scopes keys to the member, serializes local
creation, and keeps one cancellation per order.

## Request annotations are not activated

`OrderCancelRequest` declares validation annotations, but the controller omits
`@Valid`. Null or blank values can therefore reach persistence, where the
non-null idempotency constraint fails late. The target rejects missing, blank,
or overlong boundary values with HTTP 400.

## Cron routes are public

Legacy retry and auto-approval routes have no authentication. This migration
preserves the HTTP surface for compatibility but only mutates process-local
state. Production scheduler authentication is intentionally not added here.

## Broker retries can repeat indefinitely

Legacy `REQUESTED` and `APPROVED` rows publish refund events every 15 minutes
until another consumer marks them complete. The local target records one point
ledger transaction and completes the cancellation synchronously, so subsequent
retry runs do not credit it again.

## Group-purchase success time was absent from the initial target model

The earlier Rust slice did not expose a separate `succeededAt`. Reaching exact
capacity now records a dedicated process-local success timestamp alongside the
aggregate and also updates its ordinary update timestamp. The cancellation
policy reads the dedicated value so later metadata updates cannot move policy
boundaries. Existing local rows with no success timestamp use the legacy
void/full-refund policy.
