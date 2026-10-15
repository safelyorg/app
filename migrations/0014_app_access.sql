DO $$
DECLARE
    app_user CONSTANT text := 'safely';
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = app_user) THEN
        RAISE NOTICE 'User % not found, skipping access setup', app_user;
        RETURN;
    END IF;

    -- Tables and id counters that exist now
    EXECUTE format('GRANT USAGE ON SCHEMA public TO %I', app_user);
    EXECUTE format('GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO %I', app_user);
    EXECUTE format('GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO %I', app_user);

    -- Evidence is append-only: it can be added and read, never changed or deleted
    EXECUTE format('REVOKE UPDATE, DELETE ON evidence FROM %I', app_user);

    -- Tables and id counters made by later migrations get the same access
    EXECUTE format('ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO %I', app_user);
    EXECUTE format('ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT USAGE, SELECT ON SEQUENCES TO %I', app_user);
END
$$;
