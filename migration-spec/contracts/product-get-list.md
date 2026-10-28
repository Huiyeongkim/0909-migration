# Seller product list contract

Evidence: `ProductController.getProductListInfo`,
`ProductService.getProductListInfo`, `ProductListInfo`, `PageResponse`,
`ProductRepositoryAdaptor.findBySellerId`, `HeaderName`, and the shared
exception handler.

- Request: `GET /api/products` with required `X-Member-Id: <UUID>` header.
- Paging uses Spring `Pageable`: `page` defaults to `0`, `size` defaults to
  `20`, and the controller default sort is `createdAt,desc`. A supplied
  `sort=createdAt,asc` reverses that direction.
- The repository filters products by `sellerId` before sorting and paging.
- Success, including no matching products: HTTP `200` and
  `{ "status": 200, "data": PageResponse<ProductListInfo>, "message": "내가 등록한 상품 목록 조회가 완료되었습니다." }`.
- Each content item has camelCase fields `productId`, `name`, `price`,
  `imageUrl`, `category`, `createdAt`, and `updatedAt`.
- Page data has camelCase fields `content`, `totalPages`, `totalElements`,
  `first`, `last`, `size`, and `numberOfElements`.
- A missing `X-Member-Id` returns HTTP `401`, `data: null`, and message
  `로그인 정보가 없습니다.`. A malformed UUID returns HTTP `400` and
  message `적절하지 않은 요청 값이 존재합니다.`.
