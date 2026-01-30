-- SquirrelDB Multi-Project Schema
-- This file is for reference; schema is applied via code in db/postgres.rs

-- Projects table
CREATE TABLE IF NOT EXISTS projects (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  name VARCHAR(255) NOT NULL UNIQUE,
  description TEXT,
  owner_id UUID NOT NULL REFERENCES admin_users(id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_projects_name ON projects(name);
CREATE INDEX IF NOT EXISTS idx_projects_owner ON projects(owner_id);

-- Project membership
CREATE TABLE IF NOT EXISTS project_members (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  user_id UUID NOT NULL REFERENCES admin_users(id) ON DELETE CASCADE,
  role VARCHAR(50) NOT NULL DEFAULT 'member',
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  UNIQUE(project_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_project_members_project ON project_members(project_id);
CREATE INDEX IF NOT EXISTS idx_project_members_user ON project_members(user_id);

-- Add project_id to documents
ALTER TABLE documents ADD COLUMN IF NOT EXISTS project_id UUID REFERENCES projects(id);
CREATE INDEX IF NOT EXISTS idx_documents_project ON documents(project_id);
CREATE INDEX IF NOT EXISTS idx_documents_project_collection ON documents(project_id, collection);

-- Add project_id to change_queue
ALTER TABLE change_queue ADD COLUMN IF NOT EXISTS project_id UUID;
CREATE INDEX IF NOT EXISTS idx_change_queue_project ON change_queue(project_id);

-- Update trigger to include project_id in notification and change_queue
CREATE OR REPLACE FUNCTION capture_document_changes()
RETURNS TRIGGER AS $$
DECLARE
  delta JSONB;
BEGIN
  IF TG_OP = 'INSERT' THEN
    INSERT INTO change_queue (project_id, collection, document_id, operation, new_data)
    VALUES (NEW.project_id, NEW.collection, NEW.id, 'INSERT', NEW.data);
    PERFORM pg_notify('doc_changes', json_build_object(
      'project_id', NEW.project_id,
      'collection', NEW.collection,
      'id', NEW.id,
      'op', 'INSERT'
    )::text);
  ELSIF TG_OP = 'UPDATE' THEN
    delta := jsonb_build_object();
    FOR key IN SELECT jsonb_object_keys(NEW.data)
    LOOP
      IF NEW.data->key IS DISTINCT FROM OLD.data->key THEN
        delta := delta || jsonb_build_object(key, NEW.data->key);
      END IF;
    END LOOP;
    INSERT INTO change_queue (project_id, collection, document_id, operation, old_data, new_data, delta)
    VALUES (NEW.project_id, NEW.collection, NEW.id, 'UPDATE', OLD.data, NEW.data, delta);
    PERFORM pg_notify('doc_changes', json_build_object(
      'project_id', NEW.project_id,
      'collection', NEW.collection,
      'id', NEW.id,
      'op', 'UPDATE'
    )::text);
  ELSIF TG_OP = 'DELETE' THEN
    INSERT INTO change_queue (project_id, collection, document_id, operation, old_data)
    VALUES (OLD.project_id, OLD.collection, OLD.id, 'DELETE', OLD.data);
    PERFORM pg_notify('doc_changes', json_build_object(
      'project_id', OLD.project_id,
      'collection', OLD.collection,
      'id', OLD.id,
      'op', 'DELETE'
    )::text);
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Recreate trigger
DROP TRIGGER IF EXISTS document_changes_trigger ON documents;
CREATE TRIGGER document_changes_trigger
AFTER INSERT OR UPDATE OR DELETE ON documents
FOR EACH ROW EXECUTE FUNCTION capture_document_changes();

-- Create default project for existing data (if admin users exist)
INSERT INTO projects (id, name, description, owner_id)
SELECT
  '00000000-0000-0000-0000-000000000000'::UUID,
  'default',
  'Default project',
  (SELECT id FROM admin_users ORDER BY created_at LIMIT 1)
WHERE EXISTS (SELECT 1 FROM admin_users)
ON CONFLICT (id) DO NOTHING;

-- Migrate existing documents to default project
UPDATE documents SET project_id = '00000000-0000-0000-0000-000000000000'
WHERE project_id IS NULL;
