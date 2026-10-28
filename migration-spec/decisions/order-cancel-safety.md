# Order cancellation safety decisions

## Approved target behavior

The target intentionally fixes five unsafe or undefined legacy behaviors.

1. Rejecting a seller-fault cancellation restores the original order from
   `CANCELLED` to `PAYMENT_COMPLETED`.
2. Buyer-fault cancellation uses `< 48 hours` for reversal,
   `>= 48 hours && < 14 days` for return, and rejects cancellation at or after
   14 days.
3. The 20 percent fee still uses legacy truncation. Shipping is capped at the
   amount remaining after the fee, so the point refund is never negative and
   fee + shipping + refund equals the paid amount.
4. Order ownership and soft-delete are checked before replay. Effective
   idempotency is `(member_id, idempotency_key)`, while each order can have at
   most one cancellation. Another member may use the same key for their own
   order.
5. Refund, point ledger, cancellation completion, buyer cancellation
   settlement, and participation decrease are one local transaction. Kafka and
   an external point service are not used.

## Consequences

- Seller approval and immediate buyer cancellation return a final
  `COMPLETED` status because the local refund is synchronous.
- Retry remains available for imported or interrupted `REQUESTED`/`APPROVED`
  rows and cannot credit an order twice.
- A future durable adapter needs unique constraints on `order_id` and
  `(member_id, idempotency_key)`, a point-ledger idempotency key, and a database
  transaction spanning all affected tables.
