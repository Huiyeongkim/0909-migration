# Internal order participant anomalies

## Confirmed buyers are excluded

`OrderStatus.participantStatuses()` contains only `PAYMENT_COMPLETED`.
Consequently a buyer disappears from this endpoint after purchase confirmation
changes the order to `CONFIRMED`, even though the purchase remains completed.
The target preserves this observable behavior for migration compatibility.

## Members are not deduplicated

The legacy JPQL query selects `memberId` without `DISTINCT`. A member with more
than one paid order for the same group purchase therefore appears more than
once. The target preserves duplicates and does not promise a stable order.

## Internal route has no authentication boundary

The legacy controller requires no member or role header. The target keeps that
HTTP contract and assumes network-level internal routing is handled outside
this migration slice; it does not add an external identity integration.

## Process-local repository

The target currently reads the same process-local order repository used by
order creation. A future durable adapter must apply the same group-purchase,
status, and soft-delete predicates in its database query.
