CREATE TABLE IF NOT EXISTS order_schema.event_outbox (
    event_id UUID PRIMARY KEY,
    aggregate_type VARCHAR(64) NOT NULL,
    aggregate_id UUID NOT NULL,
    event_type VARCHAR(100) NOT NULL,
    payload JSONB NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    published_at TIMESTAMPTZ,
    attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    UNIQUE (aggregate_type, aggregate_id, event_type)
);

CREATE INDEX IF NOT EXISTS event_outbox_pending_idx
    ON order_schema.event_outbox (occurred_at)
    WHERE published_at IS NULL;
