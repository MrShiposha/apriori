CREATE TABLE IF NOT EXISTS {schema_name}.object
(
    object_id bigserial PRIMARY KEY,
    session_fk_id serial
        NOT NULL
        REFERENCES {schema_name}.session(session_id)
        ON DELETE CASCADE,
    layer_fk_id serial
        NOT NULL
        REFERENCES {schema_name}.layer(layer_id)
        ON DELETE CASCADE,
    object_name varchar(50) NOT NULL,
    radius real NOT NULL,
    color integer NOT NULL,
    mass real NOT NULL,
    compute_step bigint NOT NULL,

    UNIQUE (session_fk_id, object_name)
);

CREATE OR REPLACE FUNCTION {schema_name}.add_object(
    session_id integer,
    layer_id integer,
    object_name varchar(50),
    radius real,
    color integer,
    mass real,
    compute_step bigint
) RETURNS bigint
AS $$
    DECLARE
        new_object_id bigint;
    BEGIN
        INSERT INTO {schema_name}.object
        VALUES(
            DEFAULT,
            session_id,
            layer_id,
            object_name,
            radius,
            color,
            mass,
            compute_step
        ) RETURNING object_id INTO new_object_id;

       RETURN (new_object_id);
    END
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION {schema_name}.last_object_id(
    session_id integer
) RETURNS bigint
AS $$
    BEGIN
        RETURN (
            SELECT MAX(object_id)
            FROM {schema_name}.object
            WHERE session_fk_id = session_id
        );
    END
$$ LANGUAGE plpgsql;

CREATE OR REPLACE PROCEDURE {schema_name}.rename_object(
    session_id integer,
    obj_id bigint,
    new_name varchar(50)
) AS $$
    BEGIN
        UPDATE {schema_name}.object
        SET object_name = new_name
        WHERE session_fk_id = session_id AND object_id = obj_id;
    END
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION {schema_name}.is_object_exists(
    session_id integer,
    obj_name varchar(50)
) RETURNS boolean
AS $$
    BEGIN
        RETURN (
            SELECT EXISTS (
                SELECT 1 FROM {schema_name}.object
                WHERE session_fk_id = session_id AND object_name = obj_name
            )
        );
    END
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION {schema_name}.current_objects_delta(
    active_layer_id integer,
    known_objects_ids bigint[]
) RETURNS TABLE(
    out_object_id bigint,
    out_layer_fk_id integer,
    out_object_name varchar(50),
    out_radius real,
    out_color integer,
    out_mass real,
    out_compute_step bigint
) AS $$
    BEGIN
        RETURN QUERY
        WITH layers AS (
            SELECT
                layer_id
            FROM
                {schema_name}.layer_ancestors(active_layer_id)
        )
        SELECT
            object_id,
            layer_fk_id,
            object_name,
            radius,
            color,
            mass,
            compute_step
        FROM {schema_name}.object
        WHERE object_id = ANY(
            SELECT
                object_id
            FROM {schema_name}.object
            INNER JOIN layers
                ON layer_fk_id = layers.layer_id
            EXCEPT
            SELECT UNNEST(known_objects_ids)
        );
    END
$$ LANGUAGE plpgsql;