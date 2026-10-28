# PostgreSQL transaction hardening

## Scope

- Persist group purchases, likes, carts, orders, points, settlements, and cancellations in PostgreSQL.
- Keep order creation, capacity changes, point debits, expiration, and refunds in one database transaction.
- Preserve member-scoped idempotency for order and point transaction identifiers.

## Concurrency decisions

- Lock a group-purchase row before checking and changing capacity.
- Lock cart rows and then group-purchase rows in stable identifier order for multi-cart orders.
- Lock the order row first for both point payment and expiration so only one transition commits.
- Use a transaction-scoped advisory lock for an order request key before replay checks.
- Store point balance and ledger changes in the same transaction as the order transition.

## Verification

- Existing in-memory contract tests remain available as fast unit tests.
- `TEST_DATABASE_URL` enables a real PostgreSQL test that races two last-slot orders and races point payment against expiration.
- The PostgreSQL-enabled full suite passes with 88 tests.

## Known boundary

Event publication is intentionally excluded here. The next migration unit adds a transactional outbox so committed state and publishable events cannot diverge.
