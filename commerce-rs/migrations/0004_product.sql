CREATE SCHEMA IF NOT EXISTS product_schema;

CREATE TABLE IF NOT EXISTS product_schema.product (
    product_id UUID PRIMARY KEY,
    idempotency_key VARCHAR(255) NOT NULL UNIQUE,
    name VARCHAR(100) NOT NULL,
    price BIGINT NOT NULL,
    category VARCHAR(255) NOT NULL,
    description TEXT NOT NULL,
    stock INTEGER NOT NULL,
    original_url VARCHAR(255),
    image_url VARCHAR(512),
    seller_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ,
    deleted_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS product_seller_created_idx
    ON product_schema.product (seller_id, created_at DESC);
