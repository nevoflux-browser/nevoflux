-- Memory chunks table for storing embeddings and text chunks
CREATE TABLE IF NOT EXISTS memory_chunks (
    id TEXT PRIMARY KEY,
    content TEXT NOT NULL,
    embedding BLOB,
    metadata TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    session_id TEXT,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_memory_chunks_created_at ON memory_chunks(created_at);
CREATE INDEX IF NOT EXISTS idx_memory_chunks_session_id ON memory_chunks(session_id);

-- FTS5 virtual table for full-text search on memory chunks
CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
    content,
    content='memory_chunks',
    content_rowid='rowid'
);

-- Triggers to keep FTS index in sync with memory_chunks table
CREATE TRIGGER IF NOT EXISTS memory_fts_insert AFTER INSERT ON memory_chunks BEGIN
    INSERT INTO memory_fts(rowid, content) VALUES (new.rowid, new.content);
END;

CREATE TRIGGER IF NOT EXISTS memory_fts_delete AFTER DELETE ON memory_chunks BEGIN
    INSERT INTO memory_fts(memory_fts, rowid, content) VALUES('delete', old.rowid, old.content);
END;

CREATE TRIGGER IF NOT EXISTS memory_fts_update AFTER UPDATE ON memory_chunks BEGIN
    INSERT INTO memory_fts(memory_fts, rowid, content) VALUES('delete', old.rowid, old.content);
    INSERT INTO memory_fts(rowid, content) VALUES (new.rowid, new.content);
END;
