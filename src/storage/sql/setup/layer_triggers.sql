CREATE OR REPLACE FUNCTION {schema_name}.remove_layer_children()
RETURNS trigger
AS $$
    BEGIN
        DELETE FROM {schema_name}.layer
        WHERE layer_id = OLD.child_layer_id;

        RETURN OLD;
    END
$$ LANGUAGE plpgsql;

DO
$$
BEGIN
	IF NOT EXISTS(
		SELECT * FROM information_schema.triggers
		WHERE event_object_schema = '{schema_name}'
		AND event_object_table = 'layer_family'
		AND trigger_name = 'trigger_remove_layer_children'
	) THEN
		CREATE TRIGGER trigger_remove_layer_children
		AFTER DELETE
		    ON {schema_name}.layer_family
            FOR EACH ROW
		EXECUTE FUNCTION {schema_name}.remove_layer_children();
	END IF;
END
$$;