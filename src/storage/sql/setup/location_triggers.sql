CREATE OR REPLACE FUNCTION {schema_name}.remove_collision_velocity()
RETURNS trigger
AS $$
    BEGIN
        IF NOT EXISTS(
            SELECT 1 FROM {schema_name}.collision_partners
            WHERE location_fk_id=OLD.location_fk_id
        ) THEN
            UPDATE {schema_name}.location
            SET
                vcx = NULL,
                vcy = NULL,
                vcz = NULL
            WHERE
                location_id = OLD.location_fk_id;
        END IF;

        IF NOT EXISTS(
            SELECT 1 FROM {schema_name}.collision_partners
            WHERE location_fk_id=OLD.partner_fk_id
        ) THEN
            UPDATE {schema_name}.location
            SET
                vcx = NULL,
                vcy = NULL,
                vcz = NULL
            WHERE
                location_id = OLD.partner_fk_id;
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
		AND event_object_table = 'collision_partners'
		AND trigger_name = 'trigger_remove_collision_velocity'
	) THEN
		CREATE TRIGGER trigger_remove_collision_velocity
		AFTER DELETE
		    ON {schema_name}.collision_partners
            FOR EACH ROW
		EXECUTE FUNCTION {schema_name}.remove_collision_velocity();
	END IF;
END
$$;