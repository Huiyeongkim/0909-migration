# Order read contract

Sources: legacy `OrderController`, `OrderQueryService`, `OrderDetailInfo`,
`OrderInfo`, order repositories, and the shared page and response envelopes.

All routes require `X-Member-Id` as a UUID. A missing header returns HTTP 401
`로그인 정보가 없습니다.` and an invalid UUID returns HTTP 400 with the
standard invalid-parameter response. These read routes do not contact an
external member service.

## Detail

- `GET /api/orders/{orderId}`
- Only a non-deleted order is visible.
- The requester must be either the buyer (`memberId`) or seller (`sellerId`).
- A missing or soft-deleted order returns HTTP 404 `주문을 찾을 수 없습니다.`.
- Any other requester receives HTTP 403 `본인의 주문내역만 조회할 수 있습니다.`.
- Success is HTTP 200 with `주문 상세 조회가 완료 되었습니다.` and fields
  `orderId`, `orderNumber`, `quantity`, unit `price`, `status`, `memberId`,
  delivery fields, `sellerId`, `groupPurchaseId`, and timestamps.

## Buyer list

- `GET /api/orders/consumer`
- Filters non-deleted orders by the header member as buyer.
- Defaults to page 0, size 20, and `createdAt,desc`.
- `sort=createdAt,asc` and `sort=createdAt,desc` are supported.
- Success is HTTP 200 with `주문 목록 조회(구매자)가 완료 되었습니다.`.

## Seller list

- `GET /api/orders/seller`
- Filters non-deleted orders by the header member as seller.
- Defaults to page 0 and size 20.
- The legacy service replaces every requested sort with `createdAt,asc`; the
  target preserves that behavior, including when a descending sort is sent.
- Success is HTTP 200 with `주문 목록 조회(판매자)가 완료 되었습니다.`.

Both lists use the shared page fields `content`, `totalPages`, `totalElements`,
`first`, `last`, `size`, and `numberOfElements`. Each item exposes `orderId`,
`orderNumber`, `status`, `quantity`, unit `price`, computed `totalAmount`,
`groupPurchaseId`, nullable `groupPurchaseName`, and `createdAt`. A referenced
group purchase that is absent from the local repository leaves
`groupPurchaseName` as `null`, matching the legacy map lookup.
