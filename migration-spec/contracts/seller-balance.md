# Seller balance contract

- `POST /internal/balances` accepts `{sellerId}` and returns HTTP 200 with `{sellerId}` and `seller balance 생성되었습니다.`. A new balance starts at zero.
- Missing `sellerId` returns 400 `SellerId 값이 없습니다.`. Only one balance may exist for a member; duplicate creation uses the legacy generic 500 response.
- `GET /api/balances` requires UUID header `X-Member-Id` and returns `{sellerBalanceId,memberId,balance}` with `조회되었습니다.`. A member without a balance receives 404 `판매자를 찾을 수 없습니다.`.
- `GET /api/balances/history` requires the same member header and returns only that member's credit/debit history in the common page envelope.
- History fields are `sellerBalanceHistoryId`, `memberId`, nullable `sellerPayoutId`, nullable `orderSettlementId`, `amount`, `status`, and `createdAt`.
- Paging defaults to page 0 and size 20. Explicit sorting accepts `createdAt`, `amount`, `status`, or `sellerBalanceHistoryId` with `asc` or `desc`.
