CREATE SCHEMA IF NOT EXISTS settlement_schema;

CREATE TABLE IF NOT EXISTS settlement_schema.seller_balance (
    balance_id UUID PRIMARY KEY,
    member_id UUID NOT NULL UNIQUE,
    settlement_balance BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS settlement_schema.seller_balance_history (
    history_id UUID PRIMARY KEY,
    member_id UUID NOT NULL,
    seller_payout_id UUID,
    order_settlement_id UUID UNIQUE,
    amount BIGINT NOT NULL,
    status VARCHAR(10) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT seller_balance_history_status_check
        CHECK (status IN ('CREDIT', 'DEBIT'))
);

CREATE INDEX IF NOT EXISTS seller_balance_history_member_created_idx
    ON settlement_schema.seller_balance_history (member_id, created_at DESC);
