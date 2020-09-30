CREATE TABLE IF NOT EXISTS {schema_name}.session
(
	session_id serial PRIMARY KEY,
	session_name varchar(50) UNIQUE,
	last_access timestamptz NOT NULL,
	is_locked boolean NOT NULL
);

CREATE OR REPLACE FUNCTION is_session_hanged(
    session_last_access timestamptz
) RETURNS boolean
AS $$
    BEGIN
        RETURN (
            (SELECT EXTRACT (EPOCH FROM (now() - session_last_access)) > {session_max_hang_time})
        );
    END
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION {schema_name}.create_new_session(name varchar(50))
RETURNS integer
AS $$
    DECLARE
        new_session_id integer;
    BEGIN
        INSERT INTO {schema_name}.session
        VALUES(DEFAULT, name, now(), true)
        RETURNING session_id INTO new_session_id;

        INSERT INTO {schema_name}.layer
        VALUES(DEFAULT, new_session_id, 'main', 0);

        RETURN new_session_id;
    END
$$ LANGUAGE plpgsql;

CREATE OR REPLACE PROCEDURE {schema_name}.update_session_access_time(id integer)
AS $$
    BEGIN
        UPDATE {schema_name}.session
        SET last_access = now()
        WHERE session_id = id;
    END
$$ LANGUAGE plpgsql;

CREATE OR REPLACE PROCEDURE {schema_name}.unlock_session(id integer)
AS $$
    BEGIN
        UPDATE {schema_name}.session
        SET is_locked = false
        WHERE session_id = id;
    END
$$ LANGUAGE plpgsql;

CREATE OR REPLACE PROCEDURE {schema_name}.save_session(id integer, name varchar(50))
AS $$
    BEGIN
        UPDATE {schema_name}.session
        SET session_name = name
        WHERE session_id = id;
    END
$$ LANGUAGE plpgsql;

CREATE OR REPLACE PROCEDURE {schema_name}.rename_session(old_name varchar(50), new_name varchar(50))
AS $$
    BEGIN
        UPDATE {schema_name}.session
        SET session_name = new_name
        WHERE session_name = old_name;

        IF (NOT FOUND) THEN
            RAISE 'session `%` not found', old_name;
        END IF;
    END
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION {schema_name}.load_session(name varchar(50))
RETURNS integer
AS $$
    DECLARE
        load_session_id integer;
    BEGIN
        UPDATE {schema_name}.session
        SET last_access = now(), is_locked = true
        WHERE session_name = name AND is_locked = false
        RETURNING session_id INTO load_session_id;

        IF (NOT FOUND) THEN
            RAISE 'session `%` is either locked or not exists', name;
        END IF;

        RETURN load_session_id;
    END
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION {schema_name}.get_session_name(id integer)
RETURNS varchar(50)
AS $$
    BEGIN
        RETURN (
            SELECT session_name
            FROM {schema_name}.session
            WHERE session_id = id
        );
    END
$$ LANGUAGE plpgsql;

CREATE OR REPLACE PROCEDURE {schema_name}.delete_session(name varchar(50))
AS $$
    BEGIN
        DELETE FROM {schema_name}.session
        WHERE session_name = name AND is_locked = false;

        IF (NOT FOUND) THEN
            RAISE 'session `%` is either locked or not exists', name;
        END IF;
    END
$$ LANGUAGE plpgsql;