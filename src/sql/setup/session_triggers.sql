CREATE OR REPLACE PROCEDURE {schema_name}.unlock_hanged_sessions()
AS $$
	BEGIN
		UPDATE {schema_name}.session 
		SET is_locked=false
		WHERE is_locked=true 
		AND ((SELECT EXTRACT (EPOCH FROM (now() - last_access)) > {session_max_hang_time}));
	END 
$$ LANGUAGE plpgsql;

CREATE OR REPLACE PROCEDURE {schema_name}.delete_session_dependencies(id integer)
AS $$
    BEGIN
        --- TODO
		-- SELECT object_id
		-- INTO object_id_to_rm
		-- FROM {schema_name}.object
		-- WHERE object_session_id=id;

		-- DELETE FROM {schema_name}.location
		-- WHERE fk_object_id=object_id_to_rm;

		-- DELETE FROM {schema_name}.collision
		-- WHERE fk_first_object_id=object_id_to_rm;

		-- DELETE FROM {schema_name}.applied_force
		-- WHERE fk_object_id=object_id_to_rm;

		-- DELETE FROM {schema_name}.object
		-- WHERE object_id=object_id_to_rm;
		--- END TODO
    END
$$ LANGUAGE plpgsql;

CREATE OR REPLACE PROCEDURE {schema_name}.delete_session(id integer)
AS $$
    BEGIN
        CALL {schema_name}.delete_session_dependencies(id);
        DELETE FROM {schema_name}.session WHERE session_id=id;
    END
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION {schema_name}.remove_unnamed_sessions()
RETURNS trigger
AS $$
    DECLARE
        session_id_to_rm integer;
	BEGIN
        SELECT session_id INTO session_id_to_rm
        FROM {schema_name}.session
        WHERE is_locked=false AND session_name IS NULL;

        IF (session_id_to_rm IS NOT NULL) THEN
		    CALL {schema_name}.delete_session(session_id_to_rm);
        END IF;

		RETURN NEW;
	END
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION {schema_name}.unlock_hanged_sessions_on_insert_update()
RETURNS trigger
AS $$
    BEGIN
        CALL {schema_name}.unlock_hanged_sessions();

        RETURN NEW;
    END
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION {schema_name}.after_delete_session()
RETURNS trigger
AS $$
	BEGIN
        IF (NOT OLD.is_locked) THEN
            IF(OLD.session_name IS NOT NULL) THEN
                CALL {schema_name}.unlock_hanged_sessions();
            END IF;

            CALL {schema_name}.delete_session_dependencies(OLD.session_id);
        ELSE
            RAISE EXCEPTION 'Attempting to delete busy session'
            USING HINT = 'Is the session locked?';
        END IF;

		RETURN OLD;
	END
$$ LANGUAGE plpgsql;

DO
$$
BEGIN
	IF NOT EXISTS(
		SELECT * FROM information_schema.triggers
		WHERE event_object_schema = '{schema_name}' 
		AND event_object_table = 'session'
		AND trigger_name = 'trigger_unlock_hanged_sessions_insert_update'
	) THEN
		CREATE TRIGGER trigger_unlock_hanged_sessions_insert_update 
		AFTER INSERT OR UPDATE OF last_access
		ON {schema_name}.session
		EXECUTE FUNCTION {schema_name}.unlock_hanged_sessions_on_insert_update();
	END IF;
END
$$;

DO
$$
BEGIN
	IF NOT EXISTS(
		SELECT * FROM information_schema.triggers
		WHERE event_object_schema = '{schema_name}' 
		AND event_object_table = 'session'
		AND trigger_name = 'trigger_sessions_delete'
	) THEN
		CREATE TRIGGER trigger_sessions_delete 
		AFTER DELETE
		ON {schema_name}.session
        FOR EACH ROW
		EXECUTE FUNCTION {schema_name}.after_delete_session();
	END IF;
END
$$;

DO 
$$
BEGIN
	IF NOT EXISTS(
		SELECT * FROM information_schema.triggers
		WHERE event_object_schema = '{schema_name}'
		AND event_object_table = 'session'
		AND trigger_name = 'trigger_remove_unnamed_sessions'
	) THEN
		CREATE TRIGGER trigger_remove_unnamed_sessions
		AFTER INSERT OR UPDATE OF is_locked
		ON {schema_name}.session
		EXECUTE FUNCTION {schema_name}.remove_unnamed_sessions();
	END IF;
END
$$;