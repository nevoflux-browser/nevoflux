-- Add embedding column to knowledge table for vector search
ALTER TABLE knowledge ADD COLUMN embedding BLOB;
