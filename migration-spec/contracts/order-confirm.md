# Order purchase confirmation contract

Sources: legacy `OrderController`, `OrderCommandService`, `Order.confirmed`,
`OrderSettlementService`, and `OrderSettlement`.

- `PATCH /api/orders/{orderId}/purchase-confirmed`
- Requires `X-Member-Id` as a UUID. A missing header returns HTTP 401
  `로그인 정보가 없습니다.` and an invalid header or path UUID returns the standard
  HTTP 400 invalid-parameter response.
- The order must exist and its buyer `memberId` must equal the header member.
  A missing order returns HTTP 404 `주문을 찾을 수 없습니다.` and another member
  receives HTTP 403 `본인의 주문내역만 조회할 수 있습니다.`.
- Only `PAYMENT_COMPLETED` can transition to `CONFIRMED`. Every other status,
  including a repeated confirmation, returns HTTP 400
  `공구 성공 상태의 주문만 구매 확정할 수 있습니다.`.
- Success is HTTP 200 with `data: null` and `구매 확정되었습니다.`.

The confirmation creates one local settlement-ledger row in the same repository
transaction. It snapshots the order, seller, and group-purchase IDs, uses
`orderAmount = paidPrice`, `platformFeeRate = 0.2`, truncates
`platformFee = paidPrice * 0.2` toward zero, and stores
`settlementAmount = orderAmount - platformFee`. Its status is `COMPLETED`,
`createdAt` is set locally, and `settledAt` remains null. This slice does not
change seller balance and contacts no PG, broker, or external settlement system.

The process-local repository lock serializes concurrent confirmations. Exactly
one caller can perform the transition and create the ledger row; a later caller
observes `CONFIRMED` and receives the invalid-status response.
