-- Sessions table
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    title TEXT,
    mode TEXT NOT NULL DEFAULT 'chat',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    pinned INTEGER NOT NULL DEFAULT 0,
    archived INTEGER NOT NULL DEFAULT 0,
    metadata JSON
);

CREATE INDEX idx_sessions_updated_at ON sessions(updated_at DESC);
CREATE INDEX idx_sessions_pinned ON sessions(pinned);

-- Messages table
CREATE TABLE messages (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    content_type TEXT DEFAULT 'text',
    created_at INTEGER NOT NULL,
    metadata JSON
);

CREATE INDEX idx_messages_session_id ON messages(session_id);
CREATE INDEX idx_messages_created_at ON messages(created_at);

-- Permissions table
CREATE TABLE permissions (
    id TEXT PRIMARY KEY,
    resource_type TEXT NOT NULL,
    action TEXT NOT NULL,
    resource_pattern TEXT NOT NULL,
    scope TEXT NOT NULL,
    granted INTEGER NOT NULL,
    session_id TEXT,
    created_at INTEGER NOT NULL,
    expires_at INTEGER
);

CREATE INDEX idx_permissions_resource ON permissions(resource_type, action);
CREATE INDEX idx_permissions_session ON permissions(session_id);

-- Config table
CREATE TABLE config (
    key TEXT PRIMARY KEY,
    value JSON NOT NULL,
    updated_at INTEGER NOT NULL
);
