# Order read anomalies

## Seller sorting ignores the request

The seller controller accepts Spring pageable sorting, but
`OrderQueryService` always replaces it with `createdAt ASC`. The target keeps
this observable behavior instead of pretending the requested sort is honored.

## Missing group purchases produce a nullable name

The legacy query collects existing group-purchase rows into a map and reads
the name with `map.get`. If an order points to a missing group purchase, the
list item is still returned with `groupPurchaseName = null`. The target keeps
the order visible and uses the same null result.

## The named legacy test does not cover order reads

`OrderQueryServiceTest` currently tests only buyer cancellation history and
seller pending-cancellation history. It has no detail, buyer-order-list, or
seller-order-list cases. Rust route tests therefore carry the focused
regression coverage for authorization, soft deletion, ordering, paging, and
missing group-purchase names.
