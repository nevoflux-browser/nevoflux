-- Rebuild memory_fts with trigram tokenizer for CJK substring matching.
-- The default unicode61 tokenizer treats continuous CJK text as a single token,
-- making it impossible to search for substrings like "编程" within "我喜欢Python编程和数据分析".
-- The trigram tokenizer supports substring matching for all languages.

-- 1. Drop old sync triggers
DROP TRIGGER IF EXISTS memory_fts_insert;
DROP TRIGGER IF EXISTS memory_fts_delete;
DROP TRIGGER IF EXISTS memory_fts_update;

-- 2. Drop old FTS table
DROP TABLE IF EXISTS memory_fts;

-- 3. Recreate FTS table with trigram tokenizer
CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
    content,
    content='memory_chunks',
    content_rowid='rowid',
    tokenize='trigram'
);

-- 4. Recreate sync triggers
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

-- 5. Rebuild index from existing data
INSERT INTO memory_fts(rowid, content)
SELECT rowid, content FROM memory_chunks;
