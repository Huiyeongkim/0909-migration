# Group-purchase write anomalies

## Update omits create's cross-field validation

The legacy update request repeats field-level validation but
`GroupPurchaseService.updateGroupPurchase` does not recheck
`minQuantity <= maxQuantity` or `startDate <= endDate`. The Rust migration
preserves that behavior rather than introducing a contract change.

## Updating to another seller's product is allowed

Create checks that the referenced product belongs to the caller. Update checks
purchase ownership and later checks only that the new product exists; it does
not check that the product belongs to the caller. The Rust implementation
preserves this asymmetry and covers it with a focused test. Tightening it needs
an explicit behavior-change decision.

## Non-SCHEDULED delete message names OPEN

Delete rejects every status other than `SCHEDULED`, including `SUCCESS` and
`FAILED`, using `공동 구매가 OPEN 상태입니다.`. This misleading legacy error is
preserved.

## Events are deferred

The legacy service publishes created, updated, and deleted events for the
search pipeline. This unit does not connect Kafka, Elasticsearch, embeddings,
or the recommendation service. Event publication is explicitly deferred to
the `event-boundaries` migration unit.

## Local transactional boundary

The local repository serializes each group-purchase write under one `RwLock`,
and product references are checked before mutation. It is process-local and is
not a durable cross-repository database transaction; durable persistence and
transaction coordination require a later storage migration.

