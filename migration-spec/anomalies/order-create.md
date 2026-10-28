# Order creation anomalies

## Global idempotency can disclose another member's order

The legacy direct-order service calls `findByIdempotenceKey(requestId)` before
member validation, while the database key is globally unique. Reusing another
member's key can therefore return that member's order. The approved target
scope is `(member_id, request_id)`; see
`decisions/order-idempotency-scope.md`.

## Nullable request fields conflict with the database schema

Both request DTOs allow `requestId` and `receiverName` to be null. The order
entity and initial migration declare `idempotency_key` and `receiver_name` as
`NOT NULL`, so accepted requests can fail only when persistence is attempted.
The target rejects null, blank, and overlong values with HTTP 400 at the input
boundary. This is a deliberate fail-fast safety difference.

## Direct-order seller comes from the caller

The legacy direct-order path stores `sellerId` from the request without
comparing it with the referenced group purchase. The cart-order path instead
uses the group purchase seller. Because the field affects settlement
ownership, the target intentionally does not preserve this behavior. The
request field remains required for HTTP compatibility, but the order always
uses the group purchase's actual seller as the order and future settlement
owner. This approved security and data-integrity divergence prevents a caller
from redirecting settlement by changing `sellerId`.

## Cart cleanup is asynchronous in the legacy service

The legacy service publishes an in-process event after saving orders, and a
listener starts a new transaction that deletes the carts. The target has no
broker and performs the existing cart model's soft-zero operation only after
all participation and order creation succeeds. A failure leaves every cart and
participation unchanged.

## Process-local persistence

Orders, carts, and group purchases are currently process-local repository
boundaries. Their locks provide deterministic single-process atomicity, not a
durable transaction across instances. A database adapter must replace this
with transactions and the composite idempotency constraint described in the
decision record.
