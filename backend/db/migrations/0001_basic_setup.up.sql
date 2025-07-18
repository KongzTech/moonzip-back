-- Create enum type for project stages
CREATE TYPE project_stage AS ENUM (
    'Created',
    'Confirmed',
    'OnStaticPool',
    'StaticPoolClosed',
    'OnCurvePool',
    'CurvePoolClosed',
    'Graduated'
);

CREATE TYPE curve_variant AS ENUM (
    'Moonzip',
    'Pumpfun'
);

CREATE DOMAIN balance AS NUMERIC(20, 0)
    CONSTRAINT balance_check CHECK (
        VALUE >= 0 AND VALUE <= 18446744073709551615
    );

CREATE TYPE static_pool_config AS (
    launch_ts bigint
);

CREATE TYPE dev_purchase AS (
    amount balance,
    lock_period bigint
);

CREATE TYPE deploy_schema AS (
    static_pool static_pool_config,
    curve_pool curve_variant,
    dev_purchase dev_purchase
);

-- Create domain type for Solana public key
CREATE DOMAIN pubkey AS BYTEA
    CONSTRAINT pubkey_check CHECK (
        LENGTH(VALUE) = 32
    );

CREATE DOMAIN keypair AS BYTEA
    CONSTRAINT keypair_check CHECK (
        LENGTH(VALUE) = 64
    );

CREATE OR REPLACE FUNCTION kp_to_pubkey(kp keypair) RETURNS pubkey AS $$
        BEGIN
                RETURN substring(kp::bytea from 33);
        END;
$$ LANGUAGE plpgsql;

CREATE TABLE mzip_keypair (
    keypair keypair PRIMARY KEY NOT NULL
);

-- Create project table
CREATE TABLE project (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner pubkey NOT NULL,
    deploy_schema deploy_schema NOT NULL,
    stage project_stage NOT NULL DEFAULT 'Created',
    static_pool_pubkey pubkey,
    curve_pool_keypair keypair,
    dev_lock_keypair keypair,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Separate table for migrations, to avoid interrupting the other project workflow.
CREATE TABLE project_migration_lock (
    id UUID PRIMARY KEY
);

CREATE OR REPLACE PROCEDURE assign_project_keypair(project_uuid UUID)
LANGUAGE plpgsql AS $$
DECLARE
    existing_key keypair;
    new_key keypair;
BEGIN
    -- First try to get existing key
    SELECT curve_pool_keypair INTO existing_key
    FROM project 
    WHERE id = project_uuid;

    -- If key exists, exit early
    IF existing_key IS NOT NULL THEN
        RETURN;
    END IF;

    -- Otherwise get first available key from mzip_keypair and delete it
    WITH deleted_key AS (
        DELETE FROM mzip_keypair
        WHERE keypair IN (
            SELECT keypair 
            FROM mzip_keypair 
            LIMIT 1
        )
        RETURNING keypair
    )
    SELECT keypair INTO STRICT new_key
    FROM deleted_key;

    -- Update project with new key
    UPDATE project
    SET curve_pool_keypair = new_key
    WHERE id = project_uuid;
END;
$$;


CREATE TABLE token_image (
    project_id UUID PRIMARY KEY REFERENCES project(id) ON DELETE CASCADE,
    image_content BYTEA NOT NULL
);

CREATE TABLE token_meta (
    project_id UUID PRIMARY KEY REFERENCES project(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    symbol VARCHAR(12) NOT NULL,
    description TEXT NOT NULL,
    website TEXT,
    twitter TEXT,
    telegram TEXT,
    deployed_url VARCHAR(255)
);

-- Create index on commonly queried fields
CREATE INDEX idx_project_owner ON project(owner);
CREATE INDEX idx_project_created_at ON project(created_at);

CREATE TABLE user_info
(
    wallet_address    pubkey NOT NULL PRIMARY KEY UNIQUE,
    username          VARCHAR(255) NOT NULL UNIQUE,
    display_name      VARCHAR(255),
    image_url         VARCHAR(255),
    nft_address       pubkey,
    last_active       BIGINT,
    created_at        TIMESTAMPTZ  NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at        TIMESTAMPTZ
);

create index idx_user_info_wallet_address on user_info (wallet_address);
create index idx_user_info_username on user_info (username);

--- CHAIN STATES
CREATE TYPE static_pool_state AS (
    collected_lamports balance
);

CREATE TABLE static_pool_chain_state (
    project_id UUID PRIMARY KEY REFERENCES project(id) ON DELETE CASCADE,
    state static_pool_state
);

CREATE TYPE pumpfun_curve_state AS (
    virtual_sol_reserves balance,
    virtual_token_reserves balance
);

CREATE TABLE pumpfun_chain_state (
    mint pubkey PRIMARY KEY NOT NULL,
    state pumpfun_curve_state
);
