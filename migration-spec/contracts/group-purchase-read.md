# Group-purchase read contract

## Routes

- `GET /api/purchases/{purchaseId}` returns `GroupPurchaseDetailInfo` with message `공동구매가 상세 조회되었습니다.`.
- `GET /api/purchases` returns a paged thumbnail list with message `공동구매 목록 조회되었습니다.`.
- `GET /api/purchases/seller/{sellerId}` filters by seller and returns message `공동구매 판매자별 목록 조회되었습니다` (no final period).
- List defaults are zero-based page `0` and size `10`. The legacy controller declares no default sort, while caller-supplied pageable sorting is accepted.
- Read routes do not require a member header. Invalid UUIDs and malformed paging values return HTTP 400 with `적절하지 않은 요청 값이 존재합니다.`.

## Detail

- The response includes the group-purchase identifiers, quantities, title, description, discounted price, dates, seller, image, status and creation time.
- Product `price`, `originalUrl`, and `category` are joined by `productId`.
- A missing group purchase returns HTTP 404 `공동구매를 찾을 수 없습니다.`.
- A dangling product reference returns HTTP 404 `상품을 찾을 수 없습니다.`.

## Lists

- Public listing is unrestricted by status and therefore includes `SCHEDULED`, `OPEN`, `SUCCESS`, and `FAILED` rows.
- Seller listing filters only by the seller UUID. An empty result is an HTTP 200 page.
- Each thumbnail includes quantities, title, discounted price, product original price, dates, product category, status, image, and creation time.
- Page metadata uses `content`, `totalPages`, `totalElements`, `first`, `last`, `size`, and `numberOfElements`.
- If any listed group purchase references a missing product, the whole request returns the same product-not-found error as the legacy service mapping.

Sources: legacy `GroupPurchaseController`, `GroupPurchaseService`, detail and thumbnail DTOs, repository adapters, entities, and focused controller/service tests.
