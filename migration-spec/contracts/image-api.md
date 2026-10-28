# Product and group-purchase image API contract

## Routes

- Product images use `POST /api/products/images/presigned-url`, `POST /api/products/images`, and `DELETE /api/products/images?imageUrl=...`.
- Group-purchase images use the same operations under `/api/purchases/images`.
- Every controller route requires a UUID `X-Member-Id`. A missing header is 401 and an invalid UUID is 400.
- Successful JSON responses keep the legacy `{status,data,message}` envelope and messages.

## Uploads

- A presigned request contains `fileName` and `contentType`; its response contains `presignedUrl`, `imageUrl`, and `expirationSeconds` equal to 600.
- Object keys remain `products/{memberId}/{uuid}_{originalFileName}` or `group-purchases/{memberId}/{uuid}_{originalFileName}`.
- Direct upload uses multipart field `file` and rejects empty files, files over 10 MiB, and content types other than JPEG, PNG, or WebP.
- The local presigned URL accepts one matching PUT during its ten-minute lifetime.
- Uploaded bytes are served from the returned local image URL.

## Persistence and deletion

- Image bytes, owner UUID, namespace, content type, object key, and timestamps are stored in `product_schema.image_object` through the image repository.
- Delete is idempotent for an owned image URL. A member cannot delete another member's image.
