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
        IF (
            NOT EXISTS(
                SELECT * FROM {schema_name}.session
                WHERE session_name = new_name
            )
        ) THEN
            UPDATE {schema_name}.session
            SET session_name = new_name
            WHERE session_name = old_name;
        ELSE
            RAISE 'session `%` already exists', new_name;
        END IF;
    END
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION {schema_name}.load_session(name varchar(50))
RETURNS integer
AS $$
    DECLARE
        load_session_id integer;
        load_session_name varchar(50);
        load_session_last_access timestamptz;
        is_load_session_locked boolean;
    BEGIN
        SELECT session_id, session_name, last_access, is_locked
        INTO load_session_id, load_session_name, load_session_last_access, is_load_session_locked
        FROM {schema_name}.session
        WHERE session_name = name;

        IF (load_session_id IS NULL) THEN
            RAISE 'session `%` is not found', name;
        ELSIF (is_load_session_locked AND NOT is_session_hanged(load_session_last_access)) 
        THEN
            RAISE 'unable to load locked session';
        ELSE
            UPDATE {schema_name}.session
            SET last_access = now(), is_locked = true
            WHERE session_id = load_session_id;

            RETURN load_session_id;
        END IF;
    END
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION {schema_name}.get_session(id integer)
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
    DECLARE
        del_session_id integer;
        del_session_last_access timestamptz;
        is_del_session_locked boolean;
    BEGIN
        SELECT session_id, last_access, is_locked
        INTO del_session_id, del_session_last_access, is_del_session_locked
        FROM {schema_name}.session
        WHERE session_name = name;

        IF (del_session_id IS NULL) THEN
            RAISE 'session `%` is not found', name;
        ELSIF (is_del_session_locked AND NOT is_session_hanged(del_session_last_access)) 
        THEN
            RAISE 'unable to delete locked session';
        ELSE
            DELETE FROM {schema_name}.session
            WHERE session_id = del_session_id;
        END IF;
    END
$$ LANGUAGE plpgsql;