# Cart contract

Sources: legacy `CartController`, `CartService`, request and response DTOs,
`Cart`, repository queries, and shared commerce errors.

All routes require the `X-Member-Id` UUID header. A missing header returns HTTP
401 `로그인 정보가 없습니다.` and an invalid UUID returns HTTP 400 with the
standard invalid-parameter message.

## Add

- `POST /api/carts`
- Body: required `groupPurchaseId` UUID and `quantity >= 1`.
- The referenced group purchase must exist and be `OPEN`; otherwise the result
  is HTTP 404 `공동구매를 찾을 수 없습니다.` or HTTP 400
  `유효하지 않은 공동구매입니다.`.
- An existing cart for the same member and group purchase accumulates the
  quantity, including a cart previously set to zero. Otherwise a new cart is
  created.
- Success is HTTP 201 with `CartInfo` and
  `장바구니에 공동구매상품이 추가되었습니다.`.

## Update and delete

- `PATCH /api/carts` accepts required `cartId` and `quantity >= 1`, replacing
  the stored quantity. It does not recheck group-purchase status.
- `DELETE /api/carts` accepts required `cartId` in the JSON body and sets the
  quantity to zero rather than removing the row.
- Missing carts return HTTP 404
  `장바구니에서 해당 공동구매를 찾을 수 없습니다.`. A different member
  receives HTTP 403 `카트에 대한 권한이 없습니다.`.

## List and flush

- `GET /api/carts` accepts optional zero-based `page` and positive `size`
  (defaults 0 and 20) and returns the shared response envelope containing a
  `PageResponse` shape.
- Only the caller's carts with quantity greater than zero appear. With no sort
  specified, the local adapter preserves insertion order.
- `DELETE /api/carts/all` sets every cart belonging to the caller to zero and
  succeeds when there are no carts.

`CartInfo` contains `cartId`, `memberId`, `groupPurchaseId`, `quantity`,
`createdAt`, and nullable `updatedAt`.
