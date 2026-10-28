# Internal order participant contract

Sources: legacy `OrderInternalController`, `OrderQueryService`,
`OrderRepository`, `OrderJpaRepository`, and `OrderStatus.participantStatuses`.

## Route

- `GET /internal/orders/{groupPurchaseId}/participants`
- No member or role header is required by the legacy controller.
- Success is HTTP 200 with a bare JSON array of member UUIDs. It does not use
  the shared response envelope.
- An unknown group purchase is not an error; it produces `[]` because the
  legacy query reads orders directly.
- An invalid UUID path value is HTTP 400 with the standard error envelope and
  `적절하지 않은 요청 값이 존재합니다.`.

## Selection

A UUID is returned for each order whose:

- `groupPurchaseId` equals the path value,
- status is exactly `PAYMENT_COMPLETED`, and
- `deletedAt` is null.

`CONFIRMED`, `PENDING`, failed, expired, and cancelled orders are excluded.
The query selects order rows rather than distinct members, so multiple paid
orders from one member produce duplicate UUIDs. No ordering is guaranteed
because the legacy query has no `ORDER BY` clause.

The target reads the shared process-local `OrderRepository`. It adds no member,
payment, broker, or other external integration.
