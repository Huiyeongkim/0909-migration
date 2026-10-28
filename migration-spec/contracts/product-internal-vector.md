# Internal product vector feed contract

Source: `InternalProductController.getProducts`,
`ProductService.findAllProductsForVector`, `ProductInfoForVector`, and the
shared `PageResponse`.

## Request

- `GET /internal/products/vector`
- No member header or authentication input is required.
- `page` and `size` are required integer query parameters.
- `page` is zero based and `size` must be greater than zero.
- The controller always orders by `productId` ascending. Callers cannot select
  another sort.

## Success response

The endpoint returns HTTP `200` with a direct page object; it is not wrapped in
the public `ResponseDto` envelope.

```json
{
  "content": [
    {
      "productId": "00000000-0000-0000-0000-000000000001",
      "name": "상품명",
      "description": "상품 설명",
      "category": "FOOD"
    }
  ],
  "totalPages": 1,
  "totalElements": 1,
  "first": true,
  "last": true,
  "size": 20,
  "numberOfElements": 1
}
```

- `category` is the enum name, such as `FOOD`.
- The repository query is unscoped and includes every product returned by
  `findAll`, including a soft-deleted product because the legacy entity has no
  automatic deleted-row filter.
- An empty result returns `content: []`, `totalPages: 0`, `totalElements: 0`,
  `first: true`, `last: true`, and preserves the requested `size`.

## Invalid input

- A non-integer `page` or `size` is handled by the shared type-mismatch handler
  as HTTP `400` with the standard error envelope and
  `적절하지 않은 요청 값이 존재합니다.`.
- A missing required parameter reaches the legacy generic exception handler and
  returns HTTP `500` with the Spring missing-parameter message.
- A negative `page` or a `size` below one is rejected by `PageRequest` and the
  legacy generic exception handler returns HTTP `500` with the corresponding
  Spring Data assertion message.

The Rust endpoint preserves these observable statuses and response shapes.

