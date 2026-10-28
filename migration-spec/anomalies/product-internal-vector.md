# Internal product vector feed anomalies

## Invalid paging is partly reported as an internal error

The legacy controller constructs `PageRequest` directly and its global handler
maps uncaught exceptions to HTTP `500`. Missing `page` or `size`, negative
`page`, and non-positive `size` are therefore internal errors, while a value
that cannot be converted to `int` is HTTP `400`. The Rust migration preserves
this inconsistency for parity instead of silently changing the contract.

## Soft-deleted products remain visible

`ProductService.findAllProductsForVector` delegates to the repository's
unfiltered `findAll`. The legacy product entity does not declare an automatic
soft-delete filter, so rows with `deletedAt` can remain in the vector feed. The
local Rust repository likewise does not filter its soft-deleted entries.

## Local feed only

The migrated route reads the existing local product repository and returns the
DTO page. It does not call Elasticsearch, an embedding provider, the
recommendation service, or any other external system. Adding durable product
storage or an external indexing pipeline belongs to a later migration unit.

