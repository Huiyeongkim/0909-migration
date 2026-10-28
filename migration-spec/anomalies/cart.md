# Cart anomalies

## Validation differs by operation

Add requires an `OPEN` group purchase, but update changes quantity without
checking whether the purchase is still open. Add also does not compare the cart
quantity with remaining group-purchase capacity; capacity is enforced during
order participation. Both behaviors are preserved.

## Delete is soft-zero

Single delete and flush keep the row and set quantity to zero. The legacy has a
separate cleanup operation for hard deletion, which is outside this HTTP slice.
Flush uses a bulk update and therefore does not change `updatedAt`; the local
adapter preserves that difference from single delete.

## Concurrent add has no durable uniqueness constraint

The legacy table does not declare a unique constraint for member and group
purchase, so concurrent inserts can create duplicate carts. The process-local
adapter serializes access in one instance, but it is not a durable database
guarantee and must not be treated as one.

## No explicit default ordering

The legacy member-list query has no `ORDER BY`. The local adapter keeps
insertion order for deterministic tests without promising it as a public sort
contract.
