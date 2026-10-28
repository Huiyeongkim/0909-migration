CREATE SCHEMA IF NOT EXISTS product_schema;

CREATE TABLE IF NOT EXISTS product_schema.image_object (
    object_key VARCHAR(1024) PRIMARY KEY,
    owner_id UUID NOT NULL,
    image_kind VARCHAR(32) NOT NULL,
    content_type VARCHAR(100) NOT NULL,
    image_data BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT image_object_kind_check
        CHECK (image_kind IN ('products', 'group-purchases')),
    CONSTRAINT image_object_content_type_check
        CHECK (content_type IN ('image/jpeg', 'image/png', 'image/webp')),
    CONSTRAINT image_object_size_check
        CHECK (octet_length(image_data) BETWEEN 1 AND 10485760)
);

CREATE INDEX IF NOT EXISTS image_object_owner_idx
    ON product_schema.image_object (owner_id, created_at DESC);
