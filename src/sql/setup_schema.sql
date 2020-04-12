CREATE SCHEMA IF NOT EXISTS apriori;

CREATE TABLE IF NOT EXISTS apriori.session
(
	session_id serial PRIMARY KEY,
	session_name varchar(50) UNIQUE NOT NULL,
	last_access timestamptz NOT NULL,
	is_locked boolean NOT NULL
);

CREATE OR REPLACE FUNCTION apriori.remove_hanged_sessions()
RETURNS trigger
AS $$
	BEGIN
		UPDATE apriori.session 
		SET is_locked=false
		WHERE is_locked=true 
		AND ((SELECT EXTRACT (EPOCH FROM (now() - last_access)) > {session_max_hang_time}));
		
		RETURN NEW;
	END
$$ LANGUAGE plpgsql;

DO
$$
BEGIN
	IF NOT EXISTS(
		SELECT * FROM information_schema.triggers
		WHERE event_object_schema = 'apriori' 
		AND event_object_table = 'session'
		AND trigger_name = 'check_and_remove_hanged_sessions'
	) THEN
		CREATE TRIGGER check_and_remove_hanged_sessions 
		AFTER INSERT 
		OR UPDATE OF last_access
		ON apriori.session
		EXECUTE FUNCTION apriori.remove_hanged_sessions();
	END IF;
END
$$
