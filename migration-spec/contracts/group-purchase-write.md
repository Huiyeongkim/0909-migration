# Group-purchase write contract

Sources: legacy `GroupPurchaseController`, `GroupPurchaseService`, request and
response DTOs, `GroupPurchase`, repository adapters, error codes, and focused
controller/service tests.

All three routes use the `X-Member-Id` UUID header. A missing header returns
HTTP `401` with `로그인 정보가 없습니다.` and an invalid UUID returns HTTP
`400` with `적절하지 않은 요청 값이 존재합니다.`.

## Create

- `POST /api/purchases`
- Success: HTTP `201`, message `공동 구매가 생성 되었습니다.`
- The response data contains `groupPurchaseId`, `minQuantity`, `maxQuantity`,
  `discountedPrice`, `title`, nullable `description`, `status`, nullable
  `imageUrl`, `startDate`, `endDate`, `sellerId`, `productId`, `createdAt`, and
  nullable `updatedAt`.
- New purchases always start as `SCHEDULED` with zero current quantity.

Required body fields are `minQuantity`, `maxQuantity`, `title`,
`discountedPrice`, `startDate`, `endDate`, and `productId`. Quantities must be
at least one, title must be nonblank and at most 100 characters, discounted
price must be nonnegative, and start date must be present or future. Body
constraint failures return HTTP `400` with the standard invalid-parameter
message.

After body validation, the service verifies that the referenced product exists
and belongs to `X-Member-Id`. Missing product is HTTP `404`; another seller's
product is HTTP `403`. It then requires `minQuantity <= maxQuantity` and
`startDate <= endDate`, returning HTTP `400` with `잘못된 수량입니다.` or
`잘못된 날짜 범위입니다.` respectively.

## Update

- `PATCH /api/purchases/{purchaseId}`
- Success: HTTP `200`, message `공동구매 정보가 수정되었습니다.` and the same
  data shape as create.
- Missing purchase is HTTP `404`.
- A seller other than the purchase owner receives HTTP `403` with
  `본인이 등록한 공동구매만 삭제할 수 있습니다.`.
- The referenced product must exist or the operation returns HTTP `404`.

The request has the same field-level constraints as create. When the current
status is `OPEN`, `maxQuantity`, `discountedPrice`, and `productId` must remain
unchanged; otherwise the response is HTTP `400` with
`공동 구매가 OPEN 상태입니다.`. Other fields remain editable.

## Delete

- `DELETE /api/purchases/{purchaseId}`
- Success: HTTP `200`, `data: null`, message `공동구매가 삭제되었습니다`.
- Missing purchase is HTTP `404`.
- Only `SCHEDULED` purchases can be deleted. Any other status returns HTTP
  `400` with `공동 구매가 OPEN 상태입니다.`.
- After the status check, a non-owner receives HTTP `403` with the shared
  purchase-owner message.

Create, update, and delete mutate state only through the local
`GroupPurchaseRepository` write boundary.

