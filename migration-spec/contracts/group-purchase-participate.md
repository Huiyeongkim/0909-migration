# Group-purchase participation contract

Sources: legacy `ParticipateService`, `GroupPurchaseQuantityService`, the
conditional repository update, `ParticipateServiceConcurrencyTest`, and shared
commerce error codes.

Participation is an internal commerce boundary invoked by order creation, not
a public group-purchase HTTP route. It accepts a group-purchase UUID and an
integer quantity.

## Behavior

- The quantity changes only while the purchase is `OPEN`.
- `currentQuantity + quantity` must not exceed `maxQuantity`.
- Reaching the maximum exactly changes the status to `SUCCESS` in the same
  atomic write.
- A rejected write returns `GROUP_PURCHASE_IS_REACHED` (HTTP 400,
  `공동구매 참여 인원이 최대입니다.` at the eventual HTTP boundary).
- After a successful capacity check, the referenced product must exist. A
  missing product returns `PRODUCT_NOT_FOUND` (HTTP 404) and the quantity
  change is not retained.

The Rust local repository holds one write lock across the capacity decision,
product-reference check, quantity update, and success transition. Concurrent
attempts therefore cannot make the stored quantity exceed the maximum.

The legacy order request validates positive quantities before calling this
boundary. This internal slice preserves the service/repository contract and
does not add a second validation rule.
