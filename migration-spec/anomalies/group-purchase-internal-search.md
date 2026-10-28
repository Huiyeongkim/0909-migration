# Internal group-purchase search-row anomalies

## Wrapperless internal response

Unlike public Commerce APIs, this endpoint returns the row array directly and
does not use `ResponseDto`. The Rust route preserves that shape.

## Missing joins are silently omitted

The legacy JPQL uses an inner product join, then the service rebuilds results
from rows that actually exist. Missing purchase IDs and dangling product
references therefore disappear rather than producing an error. The Rust local
join preserves that behavior.

## Duplicate inputs produce duplicate rows

The database `IN` query may return one row per entity, but the service iterates
the original request IDs and looks each one up in a map. A duplicate ID is
therefore repeated in the response. This is preserved by a focused test.
