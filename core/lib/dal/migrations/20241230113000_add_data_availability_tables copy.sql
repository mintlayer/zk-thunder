CREATE TYPE operation_status AS ENUM ('pending', 'in_progress', 'completed', 'failed');

CREATE TABLE pending_ipfs_operations (
    id UUID PRIMARY KEY,
    operation_type VARCHAR NOT NULL,
    data BYTEA NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    last_attempt TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    status operation_status NOT NULL DEFAULT 'pending',
    ipfs_hash TEXT,
    requires_mintlayer BOOLEAN NOT NULL DEFAULT false
);

CREATE TABLE pending_mintlayer_batches (
    id UUID PRIMARY KEY,
    ipfs_hashes TEXT[] NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    last_attempt TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    status operation_status NOT NULL DEFAULT 'pending',
    ipfs_hash TEXT
);

CREATE INDEX idx_pending_ipfs_operations_status ON pending_ipfs_operations(status);
CREATE INDEX idx_pending_mintlayer_batches ON pending_mintlayer_batches(status);