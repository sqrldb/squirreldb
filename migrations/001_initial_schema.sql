-- SquirrelDB Initial Schema
-- This file is for reference; schema is applied via code in db/schema.rs

-- JavaScript-friendly UUID alias
CREATE OR REPLACE FUNCTION uuid() RETURNS UUID AS $$
  SELECT gen_random_uuid();
$$ LANGUAGE SQL;

-- Documents table
CREATE TABLE IF NOT EXISTS documents (
    id UUID PRIMARY KEY DEFAULT uuid(),
    collection VARCHAR(255) NOT NULL,
    data JSONB NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_documents_collection ON documents(collection);
CREATE INDEX IF NOT EXISTS idx_documents_data ON documents USING GIN(data);

-- Change queue table
CREATE TABLE IF NOT EXISTS change_queue (
    id BIGSERIAL PRIMARY KEY,
    collection VARCHAR(255) NOT NULL,
    document_id UUID NOT NULL,
    operation VARCHAR(10) NOT NULL, -- 'INSERT', 'UPDATE', 'DELETE'
    old_data JSONB,
    new_data JSONB,
    changed_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_change_queue_collection ON change_queue(collection);
CREATE INDEX IF NOT EXISTS idx_change_queue_changed_at ON change_queue(changed_at);

-- Change capture trigger function
CREATE OR REPLACE FUNCTION capture_document_changes()
RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        INSERT INTO change_queue (collection, document_id, operation, new_data)
        VALUES (NEW.collection, NEW.id, 'INSERT', NEW.data);
        PERFORM pg_notify('doc_changes', json_build_object(
            'collection', NEW.collection,
            'id', NEW.id,
            'op', 'INSERT'
        )::text);
    ELSIF TG_OP = 'UPDATE' THEN
        INSERT INTO change_queue (collection, document_id, operation, old_data, new_data)
        VALUES (NEW.collection, NEW.id, 'UPDATE', OLD.data, NEW.data);
        PERFORM pg_notify('doc_changes', json_build_object(
            'collection', NEW.collection,
            'id', NEW.id,
            'op', 'UPDATE'
        )::text);
    ELSIF TG_OP = 'DELETE' THEN
        INSERT INTO change_queue (collection, document_id, operation, old_data)
        VALUES (OLD.collection, OLD.id, 'DELETE', OLD.data);
        PERFORM pg_notify('doc_changes', json_build_object(
            'collection', OLD.collection,
            'id', OLD.id,
            'op', 'DELETE'
        )::text);
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Create trigger
DROP TRIGGER IF EXISTS document_changes_trigger ON documents;
CREATE TRIGGER document_changes_trigger
AFTER INSERT OR UPDATE OR DELETE ON documents
FOR EACH ROW EXECUTE FUNCTION capture_document_changes();
