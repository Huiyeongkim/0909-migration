# Group-purchase participation anomalies

## Missing purchase is reported as capacity reached

The legacy conditional UPDATE returns zero rows both when a purchase is absent
and when it is closed or full. `GroupPurchaseQuantityService` maps that result
to `GROUP_PURCHASE_IS_REACHED` before it performs a lookup, so the documented
`GROUP_PURCHASE_NOT_FOUND` branch is unreachable for this operation. The Rust
boundary preserves that error precedence.

## Product failure rolls back quantity

The legacy service loads the product after increasing the quantity, but both
steps run in one transaction. A missing product rolls the update back. The
local boundary checks the product inside the same repository write section
before assigning the new quantity, producing the same final state without a
database transaction.

## Event publication is deferred

The legacy service publishes `GroupPurchaseParticipatedEvent` for Kafka/search
consumers. This slice adds no broker or external search integration. Typed
local publication belongs to the later `event-boundaries` unit.

## Order wiring is deferred

The participation boundary is crate-visible, but `order-create` does not exist
in Rust yet. Its error type and repository method carry a narrow
`#[expect(dead_code)]` with that reason. Unlike `allow`, the expectation becomes
an `unfulfilled_lint_expectations` warning once order creation uses the
boundary, forcing the follow-up unit to remove it.

The legacy concurrency test requires its database/Testcontainers fixture, so
the parent verification compiles it while deterministic Rust threads exercise
the capacity invariant locally.
