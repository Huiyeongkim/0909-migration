# Internal group-purchase search-row contract

Sources: legacy `GroupPurchaseInternalController`,
`GroupPurchaseSearchService`, `GroupPurchaseSearchRow`, and the repository
product join.

## Route

- `POST /internal/purchases/search`
- Request body: `{ "ids": [<UUID>, ...] }`; `ids` may be `null` or empty.
- Success is HTTP 200 with a bare JSON array. It does not use the shared
  `ResponseDto` envelope.

Each row contains `groupPurchaseId`, `minQuantity`, `maxQuantity`, `title`,
nullable `description`, nullable `imageUrl`, `discountedPrice`, `status`,
`startDate`, `endDate`, `createdAt`, nullable `updatedAt`, `currentQuantity`,
`productId`, `category`, `price`, nullable `originalUrl`, and `sellerId`.
Dates are serialized as UTC instant strings and status/category use their enum
names.

## Selection and ordering

- `null` or empty `ids` returns `[]`.
- Results follow request ID order, not repository order.
- Repeated request IDs repeat the same row in the response.
- Unknown group-purchase IDs are omitted.
- The repository contract is an inner join with product, so a purchase whose
  product is absent is also omitted.

The Rust implementation joins the two existing local repository boundaries.
It does not connect an external search or recommendation service.
