# Product detail lookup contract

Evidence: `ProductController.getProductInfo`, `ProductService.getProductInfo`,
`ProductDetailInfo`, `ProductRepositoryAdaptor.findById`, `ResponseDto`, and
`CustomErrorCode.PRODUCT_NOT_FOUND`.

- Request: `GET /api/products/{productId}`. `productId` is a UUID. The request
  has no required header or body.
- Success: HTTP `200` and
  `{ "status": 200, "data": ProductDetailInfo, "message": "상품 조회가 완료되었습니다." }`.
- `data` uses camelCase fields: `productId`, `name`, `price`, `category`,
  `description`, `stock`, `originalLink`, `imageUrl`, `sellerId`, `createdAt`,
  and `updatedAt`. `originalLink`, `imageUrl`, and `updatedAt` may be `null`.
- Missing product: HTTP `404` and
  `{ "status": 404, "data": null, "message": "상품을 찾을 수 없습니다." }`.
- Invalid UUID: Spring converts the path parameter before the controller and
  the shared exception handler returns HTTP `400` with message
  `적절하지 않은 요청 값이 존재합니다.`.
- Persistence lookup is a single repository `findById(productId)` call.
