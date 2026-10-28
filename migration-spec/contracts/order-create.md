# Order creation contract

Sources: legacy `OrderController`, `OrderCommandService`, direct and cart request
DTOs, `Order`, cart validation, group-purchase availability and participation,
and order repository adapters.

Both routes require `X-Member-Id` as a UUID. A missing header returns HTTP 401
`로그인 정보가 없습니다.` and an invalid UUID returns HTTP 400 with the
standard invalid-parameter message. The UUID must identify an account in the
local authentication repository; otherwise the response is HTTP 404
`회원을 찾을 수 없습니다.`. No external member service is contacted.

`requestId` and `receiverName` must be nonblank. The target validates their
database lengths (`requestId <= 255`, `receiverName <= 100`) and rejects a
violation with HTTP 400. This closes the legacy request/schema mismatch
documented in `anomalies/order-create.md`.

## Direct order

- `POST /api/orders`
- Body fields are `quantity >= 1`, `address` (5 to 100 characters),
  `addressDetail` (1 to 100), `postalCode` (1 to 50), `receiverName` (1 to
  100), `sellerId`, `groupPurchaseId`, and `requestId` (1 to 255).
- The group purchase and referenced product must exist. The purchase must be
  `OPEN`, its end time must not have passed, and adding the requested quantity
  must not exceed `maxQuantity`.
- `sellerId` remains a required UUID for request compatibility, but it is not a
  trusted ownership source. The created order always snapshots the seller from
  the referenced group purchase, including when the request value differs.
- Success is HTTP 201 with `OrderRegisterInfo` and
  `주문이 생성되었습니다.`.

## Cart order

- `POST /api/orders/cart`
- Body fields are a nonempty, duplicate-free `cartIds` array, nonblank
  `address`, `addressDetail`, `postalCode`, `receiverName`, and `requestId`.
- Every cart must exist, belong to the requesting member, and have a positive
  quantity. Missing, foreign, and empty carts return the corresponding legacy
  errors.
- Every referenced group purchase must exist, be `OPEN`, and not be past its
  end time; the complete batch must fit capacity and reference existing
  products.
- The per-order idempotency key is `requestId + "-" + cartId`. The seller is
  snapshotted from each group purchase.
- All cart validations, participations, and order inserts succeed or all are
  rolled back. Carts are set to quantity zero only after full success.
- Success is HTTP 201 with an array of `OrderRegisterInfo` and
  `장바구니 주문이 생성되었습니다.`.

## Created order

An order receives a UUID and an `ORD-yyMMdd-XXXXXXXX` number. It snapshots the
quantity, discounted unit price (`price`), total (`paidPrice = quantity *
price`), delivery fields, seller, and group purchase. Initial status is
`PENDING`; `expiredAt` is ten minutes after `createdAt`.

`OrderRegisterInfo` exposes `orderId`, `orderNumber`, `quantity`, unit `price`,
`status`, `memberId`, delivery fields, `sellerId`, `groupPurchaseId`,
`createdAt`, nullable `updatedAt`, and nullable `deletedAt`. The internal total
and expiry are intentionally not fields in this legacy registration response.

Idempotent retries are scoped to the member and return the original order
without another participation. See `decisions/order-idempotency-scope.md`.
Participation and local order persistence are synchronous; no PG, point,
Kafka, or production credential is part of this slice.
