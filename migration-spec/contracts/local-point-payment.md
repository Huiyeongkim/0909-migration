# Local point payment contract

This contract replaces Toss/PG and Kafka payment events with deterministic
loopback-only point operations. It is shared by `commerce-rs`, the React web
application, and the Tauri desktop application.

All charge and payment routes require `Authorization: Bearer <session-token>`.
The session resolves the member ID; clients never send or choose a member ID.
Missing, invalid, expired, or revoked sessions use the local-auth error
envelope.

## Development point charge

- `POST /api/points/charge`
- Request:

```json
{
  "amount": 10000,
  "transactionId": "019a24f0-5db3-7a67-80d5-37ccf39e28dd"
}
```

- `amount` must be a positive integer that does not overflow the point balance.
- `transactionId` is a UUID and is idempotent within the authenticated member.
- A replay returns the original result without adding points again.
- Success is HTTP 200 and returns:

```json
{
  "status": 200,
  "data": {
    "transactionId": "019a24f0-5db3-7a67-80d5-37ccf39e28dd",
    "amount": 10000,
    "balance": 10000
  },
  "message": "포인트가 충전되었습니다."
}
```

This is a local development funding boundary, not a production free-charge
API. The loopback bind is required. A non-local deployment must disable it or
replace it with an authorized funding source.

## Point order payment

- `POST /api/orders/{orderId}/point-payment`
- Request:

```json
{
  "transactionId": "019a24f1-3923-7816-95ff-124c28c66f40"
}
```

- The authenticated member must own the non-deleted order.
- The order must be `PENDING` and its `expiredAt` must be later than now.
- The amount is never accepted from the client. The server debits the order's
  snapshotted `paidPrice`.
- The effective idempotency key is `(member_id, transaction_id)`. A replay for
  the same order returns the original result without another debit. Reusing
  the key for another order is rejected.
- Balance debit, `USE` ledger entry, `PAYMENT_COMPLETED`, payment method
  `POINT`, and `paidAt` are one transaction.
- Insufficient points return HTTP 400, do not add a ledger row, and leave the
  order `PENDING` so the member can charge and retry before expiry.
- Success is HTTP 200 and returns:

```json
{
  "status": 200,
  "data": {
    "orderId": "019a24f2-c3cc-7ed3-b1e3-f85c01bb27ed",
    "transactionId": "019a24f1-3923-7816-95ff-124c28c66f40",
    "amount": 16000,
    "balance": 4000,
    "status": "PAYMENT_COMPLETED",
    "paymentMethod": "POINT",
    "paidAt": "2026-10-28T09:00:00Z"
  },
  "message": "포인트 결제가 완료되었습니다."
}
```

After commit, the local event adapter logs only event type, transaction ID,
order ID, and amount. It does not log the token, member ID, or delivery data.

## Order expiration

- `POST /api/orders/expire`
- The route preserves the legacy unauthenticated scheduler contract while the
  server is loopback-only.
- It selects non-deleted `PENDING` orders whose `expiredAt` is earlier than now.
- Each actual `PENDING → EXPIRED` transition and its group-purchase quantity
  decrease are atomic. A concurrently paid order is not counted as expired.
- Success is HTTP 200 with null data and
  `만료된 주문을 처리했습니다.`.

External PG failure and Kafka payment/refund event routes are intentionally not
part of the target. Point balance shortage is a retryable request failure, and
local cancellation already performs refund completion synchronously.
