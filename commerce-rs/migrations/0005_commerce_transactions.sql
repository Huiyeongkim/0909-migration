CREATE TABLE IF NOT EXISTS product_schema.group_purchase (
    group_purchase_id UUID PRIMARY KEY,
    min_quantity INTEGER NOT NULL,
    max_quantity INTEGER NOT NULL,
    title VARCHAR(100) NOT NULL,
    description TEXT,
    discounted_price BIGINT NOT NULL,
    current_quantity INTEGER NOT NULL DEFAULT 0 CHECK (current_quantity >= 0),
    like_count INTEGER NOT NULL DEFAULT 0 CHECK (like_count >= 0),
    start_date TIMESTAMPTZ NOT NULL,
    end_date TIMESTAMPTZ NOT NULL,
    seller_id UUID NOT NULL,
    product_id UUID NOT NULL REFERENCES product_schema.product(product_id),
    image_url VARCHAR(512),
    status VARCHAR(32) NOT NULL,
    succeeded_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ,
    CHECK (min_quantity > 0 AND max_quantity >= min_quantity),
    CHECK (current_quantity <= max_quantity)
);

CREATE INDEX IF NOT EXISTS group_purchase_seller_created_idx
    ON product_schema.group_purchase (seller_id, created_at DESC);

CREATE TABLE IF NOT EXISTS product_schema.group_purchase_like (
    group_purchase_like_id UUID PRIMARY KEY,
    member_id UUID NOT NULL,
    group_purchase_id UUID NOT NULL REFERENCES product_schema.group_purchase(group_purchase_id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (member_id, group_purchase_id)
);

CREATE SCHEMA IF NOT EXISTS order_schema;

CREATE TABLE IF NOT EXISTS order_schema.shopping_cart (
    shopping_cart_id UUID PRIMARY KEY,
    member_id UUID NOT NULL,
    group_purchase_id UUID NOT NULL REFERENCES product_schema.group_purchase(group_purchase_id),
    quantity INTEGER NOT NULL CHECK (quantity >= 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ,
    UNIQUE (member_id, group_purchase_id)
);

CREATE TABLE IF NOT EXISTS order_schema."order" (
    order_id UUID PRIMARY KEY,
    order_number VARCHAR(255) NOT NULL UNIQUE,
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    price BIGINT NOT NULL,
    paid_price BIGINT NOT NULL,
    status VARCHAR(32) NOT NULL,
    payment_method VARCHAR(32),
    paid_at TIMESTAMPTZ,
    member_id UUID NOT NULL,
    address VARCHAR(100) NOT NULL,
    address_detail VARCHAR(100) NOT NULL,
    postal_code VARCHAR(50) NOT NULL,
    receiver_name VARCHAR(100) NOT NULL,
    seller_id UUID NOT NULL,
    group_purchase_id UUID NOT NULL REFERENCES product_schema.group_purchase(group_purchase_id),
    idempotency_key VARCHAR(255) NOT NULL,
    expired_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ,
    deleted_at TIMESTAMPTZ,
    UNIQUE (member_id, idempotency_key)
);

CREATE INDEX IF NOT EXISTS order_member_created_idx
    ON order_schema."order" (member_id, created_at DESC);
CREATE INDEX IF NOT EXISTS order_seller_created_idx
    ON order_schema."order" (seller_id, created_at DESC);

CREATE TABLE IF NOT EXISTS order_schema.point_account (
    member_id UUID PRIMARY KEY,
    balance BIGINT NOT NULL DEFAULT 0 CHECK (balance >= 0)
);

CREATE TABLE IF NOT EXISTS order_schema.point_ledger (
    ledger_id UUID PRIMARY KEY,
    member_id UUID NOT NULL,
    transaction_id UUID NOT NULL,
    order_id UUID,
    kind VARCHAR(20) NOT NULL,
    amount BIGINT NOT NULL,
    balance_after BIGINT NOT NULL CHECK (balance_after >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (member_id, transaction_id)
);

CREATE TABLE IF NOT EXISTS order_schema.order_settlement (
    settlement_id UUID PRIMARY KEY,
    order_id UUID NOT NULL UNIQUE REFERENCES order_schema."order"(order_id),
    seller_id UUID NOT NULL,
    amount BIGINT NOT NULL,
    status VARCHAR(20) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS order_schema.canceled_order (
    cancellation_id UUID PRIMARY KEY,
    order_id UUID NOT NULL UNIQUE REFERENCES order_schema."order"(order_id),
    member_id UUID NOT NULL,
    seller_id UUID NOT NULL,
    idempotency_key VARCHAR(255) NOT NULL,
    reason VARCHAR(64) NOT NULL,
    detail_reason TEXT,
    original_paid_amount BIGINT NOT NULL,
    cancel_fee_amount BIGINT NOT NULL,
    shipping_fee_amount BIGINT NOT NULL,
    refund_amount BIGINT NOT NULL,
    policy_id VARCHAR(64) NOT NULL,
    policy_snapshot JSONB NOT NULL,
    status VARCHAR(32) NOT NULL,
    canceled_at TIMESTAMPTZ NOT NULL,
    returned_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ,
    UNIQUE (member_id, idempotency_key)
);
