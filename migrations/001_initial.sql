-- Users table (populated from Google SSO)
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    google_id VARCHAR(255) UNIQUE NOT NULL,
    email VARCHAR(255) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    avatar_url TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Persisted roasts
CREATE TABLE roasts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    startup_name VARCHAR(255) NOT NULL,
    startup_url TEXT NOT NULL,
    roast_text TEXT NOT NULL,
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    fire_count INT DEFAULT 0,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Votes (one per user per roast)
CREATE TABLE votes (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    roast_id UUID NOT NULL REFERENCES roasts(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    PRIMARY KEY (user_id, roast_id)
);

-- Sessions table for tower-sessions
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    data BYTEA NOT NULL,
    expiry_date TIMESTAMPTZ NOT NULL
);

-- Indexes for performance
CREATE INDEX idx_roasts_fire_count ON roasts(fire_count DESC);
CREATE INDEX idx_roasts_created_at ON roasts(created_at DESC);
CREATE INDEX idx_roasts_user_id ON roasts(user_id);
CREATE INDEX idx_votes_roast_id ON votes(roast_id);
CREATE INDEX idx_sessions_expiry ON sessions(expiry_date);
