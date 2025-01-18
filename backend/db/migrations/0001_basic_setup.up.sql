-- Create enum type for project stages
CREATE TYPE project_stage AS ENUM (
    'Created',
    'Confirmed', 
    'Prelaunch',
    'OnCurve',
    'Graduated'
);

CREATE TYPE curve_variant AS ENUM (
    'Moonzip',
    'Pumpfun'
);

CREATE TYPE deploy_schema AS (
    use_static_pool BOOLEAN,
    curve_pool curve_variant
);

-- Create composite type for token metadata
CREATE TYPE token_meta AS (
    name VARCHAR(255),
    ticker VARCHAR(12),
    description TEXT,
    image TEXT,
    website TEXT,
    twitter TEXT,
    telegram TEXT
);

-- Create domain type for Solana public key
CREATE DOMAIN pubkey AS VARCHAR(32)
    CONSTRAINT pubkey_check CHECK (
        LENGTH(VALUE) = 32
    );

-- Create project table
CREATE TABLE IF NOT EXISTS project (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner pubkey NOT NULL,
    token_meta token_meta NOT NULL,
    deploy_schema deploy_schema NOT NULL,
    stage project_stage NOT NULL DEFAULT 'Created',
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Create index on commonly queried fields
CREATE INDEX idx_project_owner ON project(owner);
CREATE INDEX idx_project_stage ON project(stage);
CREATE INDEX idx_project_created_at ON project(created_at);