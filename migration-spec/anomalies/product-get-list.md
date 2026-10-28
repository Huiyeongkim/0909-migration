# Seller product list anomalies

- The inspected `ProductControllerTest` has no seller-list case. The service
  test covers two mapped products and verifies the seller/pageable repository
  call, but does not assert the HTTP envelope or empty-page metadata.
- Spring `Pageable` can express arbitrary entity-property sorts and repeated
  `sort` parameters. This slice implements the controller's declared
  `createdAt` ordering in ascending or descending direction only; other sort
  shapes return the shared invalid-parameter response.
- The local repository compares the existing RFC 3339 timestamp strings. Data
  produced by this local application must keep one normalized offset for the
  ordering to match instant-based database ordering.
