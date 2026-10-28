# Order idempotency scope

## Decision

Order idempotency keys are scoped by `(member_id, request_id)` rather than by
`request_id` alone.

For direct orders, a retry by the same member with the same nonblank
`requestId` returns the original order without another participation. A
different member may use the same `requestId` and receives a separate order.

For cart orders, each effective key is
`(member_id, requestId + "-" + cartId)`. A retry returns the same set of orders
even after the successfully ordered carts have been set to quantity zero.

## Reason

The legacy repository looks up a globally unique idempotency key before it
validates the requesting member. That allows one member to receive another
member's order response by guessing or reusing the same key. The member-scoped
key preserves retry behavior without exposing an order across the identity
boundary.

The process-local order repository serializes the lookup, group-purchase
participation, and insert for a key, so concurrent retries by one member cannot
create multiple orders or increment participation more than once.

## Consequences

- The target intentionally does not preserve the legacy global unique-key
  behavior.
- A future durable order adapter must enforce a composite unique constraint on
  `member_id` and `idempotency_key` and perform the replay check and insert in
  one transaction.
- Partially existing cart-order keys are rejected as a conflict instead of
  silently creating only the missing subset.

## Related integrity decision: direct-order seller source

The direct-order request keeps `sellerId` as a required UUID for HTTP
compatibility, but the value is not trusted or persisted. The referenced group
purchase is the source of truth for the order seller and future settlement
owner. This applies even when the request value differs.

This intentionally diverges from the legacy direct-order implementation,
which stores the caller-provided seller without comparison. Using the aggregate
owner prevents a caller from redirecting settlement and makes direct and cart
orders use the same ownership source.
