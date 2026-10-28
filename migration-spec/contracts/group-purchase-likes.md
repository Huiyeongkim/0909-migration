# Group-purchase like contract

Sources: legacy `GroupPurchaseController`, `GroupPurchaseLikeService`,
`GroupPurchaseLike`, repository adapters, shared error codes, and controller
tests.

Both routes require the `X-Member-Id` UUID header. A missing header returns HTTP
`401` with `로그인 정보가 없습니다.`; an invalid member or purchase UUID returns
HTTP `400` with `적절하지 않은 요청 값이 존재합니다.`.

## Like

- `POST /api/purchases/{purchaseId}/likes`
- Success: HTTP `200`, `data: null`, message `공동구매가 찜 되었습니다.`.
- A missing purchase returns HTTP `404` with `공동구매를 찾을 수 없습니다.`.
- A repeated like by the same member returns HTTP `400` with
  `이미 찜한 상태입니다.`.

## Unlike

- `DELETE /api/purchases/{purchaseId}/likes`
- Success: HTTP `200`, `data: null`, message `공동구매 찜이 해제되었습니다.`.
- If that member has no like for the purchase, the response is HTTP `400` with
  `찜한 상태가 아닙니다.`.

Membership and `likeCount` change together under the local
`GroupPurchaseRepository` write boundary. The member/purchase pair is unique,
so concurrent repeated likes increment the count once.
