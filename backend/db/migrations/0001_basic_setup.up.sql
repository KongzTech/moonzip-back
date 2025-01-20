-- Create enum type for project stages
CREATE TYPE project_stage AS ENUM (
    'Created',
    'OnStaticPool',
    'StaticPoolClosed'
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

CREATE TYPE deploy_schema AS (
    use_static_pool BOOLEAN,
    curve_pool curve_variant,
    launch_after INTERVAL,
    dev_purchase balance
);

-- Create domain type for Solana public key
CREATE DOMAIN pubkey AS BYTEA
    CONSTRAINT pubkey_check CHECK (
        LENGTH(VALUE) = 32
    );

-- Create project table
CREATE TABLE project (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner pubkey NOT NULL,
    deploy_schema deploy_schema NOT NULL,
    stage project_stage NOT NULL DEFAULT 'Created',
    static_pool_mint pubkey,
    curve_pool_mint pubkey,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

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