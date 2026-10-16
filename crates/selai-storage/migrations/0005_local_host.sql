-- "This computer": a built-in target for the local terminal and for agent tasks
-- on the operator's own machine. SSH hosts are kind 'ssh'. The local row is the
-- only one of kind 'local'; it has no credentials and no host keys.
ALTER TABLE hosts ADD COLUMN kind TEXT NOT NULL DEFAULT 'ssh' CHECK (kind IN ('ssh', 'local'));

INSERT INTO hosts (id, label, hostname, port, username, auth_method, created_at_ms, updated_at_ms, kind)
SELECT '0c0c0c0c-0000-4000-8000-000000000000',
       CASE WHEN EXISTS (SELECT 1 FROM hosts
                          WHERE label = 'This computer' COLLATE NOCASE AND archived_at_ms IS NULL)
            THEN 'This computer (local)' ELSE 'This computer' END,
       'localhost', 22, 'local', 'password', 0, 0, 'local'
 WHERE NOT EXISTS (SELECT 1 FROM hosts WHERE kind = 'local');
