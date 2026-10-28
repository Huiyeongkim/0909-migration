# Order cancellation contract

Sources: legacy `CanceledOrderController`, `OrderCronController`,
`CanceledOrderService`, cancellation policies, canceled-order repositories, and
the shared response/page envelopes.

Buyer and seller routes require `X-Member-Id` as a UUID. A missing header
returns HTTP 401 `로그인 정보가 없습니다.` and a malformed value returns the
standard HTTP 400 invalid-parameter response. Cron routes intentionally keep
the legacy unauthenticated HTTP contract; production scheduling and access
control are outside this local migration.

## Request and buyer history

- `POST /api/orders/cancel/{orderId}` accepts `reason`, nonblank
  `detailReason`, and nonblank `idempotencyKey`.
- Only the non-deleted buyer's `PAYMENT_COMPLETED` order can be canceled.
- A buyer-fault `CHANGE_OF_MIND` request decreases participation and performs
  the local point refund immediately. The cancellation returned by later reads
  is `COMPLETED`.
- Seller-fault reasons `PRODUCT_DEFECT`, `DELIVERY_DELAY`, and `OUT_OF_STOCK`
  create `PENDING` cancellations. `GROUP_PURCHASE_FAILED` is not accepted by
  this public flow.
- Success is HTTP 200 with null data and `주문 취소 되었습니다.`.
- `GET /api/orders/cancel` returns the buyer's cancellation page with message
  `주문 취소 목록을 조회했습니다.`.

## Seller processing

- `GET /api/orders/cancel/pending` returns only the requesting seller's
  `PENDING` cancellations with message
  `주문 취소 대기 내역을 페이징하여 조회했습니다.`.
- `PATCH /api/orders/cancel/{orderId}/approve` requires the order's seller,
  decreases participation, performs the local point refund, and returns the
  now-`COMPLETED` cancellation with
  `판매자가 주문 취소 승인했습니다.`.
- `PATCH /api/orders/cancel/{orderId}/reject` requires the seller, marks the
  cancellation `REJECTED`, restores the order to `PAYMENT_COMPLETED`, and
  returns `판매자가 주문 취소 거부했습니다.`.

## Retry and automatic approval

- `POST /api/orders/cancel/retry` completes local refunds for `REQUESTED` or
  `APPROVED` cancellations older than 15 minutes without decreasing quantity a
  second time.
- `POST /api/orders/cancel/auto` completes `PENDING` cancellations older than
  two days, including one participation decrease and one local refund.
- Both return HTTP 200 with null data and
  `주문 취소 재처리를 실행했습니다.`.

## Cancellation information and paging

Each row exposes `orderId`, `status`, `originalPaidAmount`,
`cancelFeeAmount`, `shippingFeeAmount`, `refundAmount`, `reason`,
`detailReason`, and `createdAt`. Pages expose `content`, `totalPages`,
`totalElements`, `first`, `last`, `size`, and `numberOfElements`.

## Local transaction

The order, cancellation, point balance, auditable point ledger, and buyer-fault
fee settlement share one process-local order store. Participation decrease is
performed through a rollback-safe group-purchase boundary while the order
store is locked. A failure restores both stores. A successful refund emits a
structured log containing only event type, transaction ID, order ID, and
amount; member, delivery, and receiver data are never logged.

This is deterministic single-process atomicity. A future PostgreSQL adapter
must use one database transaction plus unique constraints for order and
member-scoped idempotency.
