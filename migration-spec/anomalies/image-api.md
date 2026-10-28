# image-api anomalies

- AWS S3, MinIO, production credentials, and production image data are not used. Returned upload and image URLs point to the loopback Rust service.
- Image bytes and metadata are durable in PostgreSQL, while outstanding presigned upload grants are local process memory and disappear on restart.
- The local adapter verifies JPEG, PNG, and WebP signatures and permits deletion only by the UUID embedded in the object key. These checks are stricter than the legacy uploader, which trusts the declared content type and ignores the member header during deletion.
- The global body limit allows roughly 11 MiB so the handler can reproduce the legacy 10 MiB validation. Larger requests may be rejected before the legacy-style error envelope is produced.
- The legacy project has no dedicated image-controller test; `testClasses` is the available regression check.
