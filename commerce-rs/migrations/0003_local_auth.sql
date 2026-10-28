CREATE SCHEMA IF NOT EXISTS auth_schema;

CREATE TABLE IF NOT EXISTS auth_schema.local_account (
    member_id UUID PRIMARY KEY,
    username VARCHAR(64) NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS auth_schema.local_session (
    token_hash BYTEA PRIMARY KEY,
    member_id UUID NOT NULL REFERENCES auth_schema.local_account(member_id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS local_session_member_idx
    ON auth_schema.local_session (member_id);
