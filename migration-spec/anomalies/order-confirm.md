# Order purchase confirmation anomalies

## Soft-deleted orders remain confirmable

The legacy command uses `findById` rather than a query that excludes
`deletedAt`. A soft-deleted `PAYMENT_COMPLETED` order can therefore be confirmed.
The target preserves this observable behavior for this migration unit and
records it rather than silently changing the lookup contract.

## Error text refers to group-purchase success

The actual precondition is order status `PAYMENT_COMPLETED`, but the legacy
message says `공구 성공 상태의 주문만 구매 확정할 수 있습니다.`. The target keeps the
message for response compatibility.

## Legacy persistence permits concurrent duplicate settlements

The legacy order has no optimistic-lock version and the settlement table has no
unique order constraint. Two database transactions can both read
`PAYMENT_COMPLETED` and insert separate settlement rows. The process-local
target serializes the state transition and ledger insertion with one lock, so a
single process cannot create duplicate ledgers. A future database adapter must
enforce the same invariant transactionally, ideally with an order-level lock or
a unique settlement constraint on `order_id`.

## Legacy focused tests do not cover the command

`OrderSettlementServiceTest` verifies settlement arithmetic and repository
delegation but does not call the purchase-confirmation controller or command.
Rust focused tests cover authorization, state transition, repeated and
concurrent calls, ledger fields, and rollback when settlement persistence is
rejected.
