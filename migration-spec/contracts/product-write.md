# Product create, update, and delete contract

Evidence: `ProductController`, `ProductService`, product request/command/info
DTOs, `Product`, `ProductRepositoryAdaptor`, `CustomErrorCode`, and the product
service/controller tests.

## Create

- `POST /api/products` requires `X-Member-Id: <UUID>` and returns HTTP `201`.
- JSON fields are `name`, `price`, `category`, `description`, `stock`, nullable
  `originalUrl`, nullable `imageUrl`, and `idempotencyKey`.
- Bean validation requires non-blank name, description, and idempotency key;
  positive price and stock; and a non-null product category.
- Success data fields are `productId`, `name`, `price`, `category`,
  `description`, `stock`, `originalUrl`, `imageUrl`, `sellerId`, and
  `createdAt`. The message is `상품이 등록되었습니다.`.
- An existing idempotency key returns the original product with the same `201`
  envelope; new request values are ignored.

## Update

- `PATCH /api/products/{productId}` has no member header in the legacy API.
- The body replaces `name`, `price`, `category`, `description`, `stock`,
  nullable `originalLink`, and nullable `imageUrl`.
- Success returns HTTP `200`, the same fields except timestamps, and message
  `상품 수정이 완료되었습니다.`. A missing product returns HTTP `404`
  with `상품을 찾을 수 없습니다.`.

## Delete

- `DELETE /api/products/{productId}` requires `X-Member-Id: <UUID>`.
- Only the product seller may delete it; another member receives HTTP `403`
  and `본인이 등록한 상품이 아닙니다.`.
- A scheduled or open group purchase blocks deletion with HTTP `409` and
  `진행 중이거나 예정된 공동구매가 존재합니다.`.
- A product used by a non-active group purchase is soft-deleted by setting
  `deletedAt`; an unused product is physically removed.
- Both successful delete paths return HTTP `200`, `data: null`, and
  `상품이 삭제되었습니다.`. A missing product returns the product `404`.

Malformed UUIDs, JSON, headers, and validated create fields use the shared HTTP
`400` envelope with `적절하지 않은 요청 값이 존재합니다.`.
