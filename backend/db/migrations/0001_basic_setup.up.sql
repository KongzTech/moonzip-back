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

CREATE TYPE deploy_schema AS (
    static_pool static_pool_config,
    curve_pool curve_variant,
    dev_purchase balance
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
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
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