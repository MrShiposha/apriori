CREATE TABLE IF NOT EXISTS {schema_name}.layer
(
    layer_id serial PRIMARY KEY,
    session_fk_id serial
        NOT NULL
        REFERENCES {schema_name}.session(session_id)
        ON DELETE CASCADE,
    layer_name varchar(50) NOT NULL,
    start_time bigint NOT NULL,

    UNIQUE(session_fk_id, layer_name)
);

CREATE TABLE IF NOT EXISTS {schema_name}.layer_family
(
    parent_layer_id serial
        NOT NULL
        REFERENCES {schema_name}.layer(layer_id)
        ON DELETE CASCADE,
    child_layer_id serial
        NOT NULL
        REFERENCES {schema_name}.layer(layer_id)
        ON DELETE CASCADE
);

CREATE OR REPLACE FUNCTION {schema_name}.main_layer_id(
    session_id integer
) RETURNS integer
AS $$
    BEGIN
        RETURN (
            SELECT MIN(layer_id)
            FROM {schema_name}.layer
            WHERE session_fk_id=session_id
        );
    END
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION {schema_name}.layer_id(
    session_id integer,
    in_layer_name varchar(50)
) RETURNS integer
AS $$
    BEGIN
        RETURN (
            SELECT layer_id FROM {schema_name}.layer
            WHERE session_fk_id = session_id AND layer_name = in_layer_name
        );
    END
$$ LANGUAGE plpgsql;

-- CREATE OR REPLACE FUNCTION {schema_name}.layer_object_ids(
--     layer_id integer
-- ) RETURNS integer[]
-- AS $$
--     BEGIN
--         RETURN (
--                 SELECT
--                     array_agg(object_id::integer ORDER BY object_id)
--                 FROM {schema_name}.object
--                 WHERE
--                     layer_fk_id = layer_id
--         );
--     END
-- $$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION {schema_name}.add_layer(
    session_id integer,
    active_layer_id integer,
    layer_name varchar(50),
    start_time bigint
) RETURNS integer
AS $$
    DECLARE
        new_layer_id integer;
    BEGIN
        INSERT INTO {schema_name}.layer
        VALUES(DEFAULT, session_id, layer_name, start_time)
        RETURNING layer_id INTO new_layer_id;

        INSERT INTO {schema_name}.layer_family
        VALUES(
            (SELECT {schema_name}.current_layer_id(active_layer_id, start_time)),
            new_layer_id
        );

        RETURN new_layer_id;
    END
$$ LANGUAGE plpgsql;

CREATE OR REPLACE PROCEDURE {schema_name}.remove_layer(
    in_layer_id integer
)
AS $$
    BEGIN
        DELETE FROM {schema_name}.layer
        WHERE layer_id=in_layer_id;
    END
$$ LANGUAGE plpgsql;

CREATE OR REPLACE PROCEDURE {schema_name}.rename_layer(
    in_layer_id integer,
    new_layer_name varchar(50)
)
AS $$
    BEGIN
        UPDATE {schema_name}.layer
        SET layer_name = new_layer_name
        WHERE layer_id = in_layer_id;
    END
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION {schema_name}.layer_children(
    session_id integer,
    in_layer_id integer
) RETURNS integer[]
AS $$
    BEGIN
        RETURN (
            SELECT
                array_agg(layer_id::integer ORDER BY start_time)
            FROM {schema_name}.layer
            INNER JOIN {schema_name}.layer_family f
                ON f.parent_layer_id = in_layer_id
            WHERE
                layer_id = f.child_layer_id
        );
    END
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION {schema_name}.layer_name(
    in_layer_id integer
) RETURNS varchar(50)
AS $$
    BEGIN
        RETURN (
            SELECT
                layer_name
            FROM
                {schema_name}.layer
            WHERE
                layer_id=in_layer_id
        );
    END
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION {schema_name}.layer_start_time(
    in_layer_id integer
) RETURNS bigint
AS $$
    BEGIN
        RETURN (
            SELECT
                start_time
            FROM
                {schema_name}.layer
            WHERE
                layer_id=in_layer_id
        );
    END
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION {schema_name}.current_layer_id(
    active_layer_id integer,
    current_vtime bigint
) RETURNS integer
AS $$
    BEGIN
        RETURN (
            SELECT
                layer_id
            FROM {schema_name}.query_layers_info(
                active_layer_id,
                current_vtime,
                current_vtime
            )
            WHERE
                current_vtime BETWEEN layer_start_time AND layer_stop_time
            ORDER BY layer_id DESC
            LIMIT 1
        );
    END
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION {schema_name}.layer_ancestors(
    in_layer_id integer
) RETURNS TABLE (
    layer_id integer,
    start_time bigint
) AS $$
#variable_conflict use_column
    BEGIN
        RETURN QUERY
            WITH RECURSIVE cte_layer AS (
                    SELECT
                        layer_id,
                        start_time
                    FROM
                        {schema_name}.layer
                    WHERE
                        layer_id = in_layer_id
                    UNION
                        SELECT
                            l.layer_id,
                            l.start_time
                        FROM
                            {schema_name}.layer l
                        INNER JOIN {schema_name}.layer_family f
                            ON f.parent_layer_id = l.layer_id
                        INNER JOIN cte_layer accum
                            ON accum.layer_id = f.child_layer_id
            ) SELECT * FROM cte_layer;
    END
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION {schema_name}.query_layers_info(
    active_layer_id integer,
    in_start_time bigint,
    in_stop_time bigint
) RETURNS TABLE (
    layer_id integer,
    layer_start_time bigint,
    layer_stop_time bigint
) AS $$
#variable_conflict use_column
    BEGIN
        RETURN QUERY
            SELECT
                layer_id,
                GREATEST(start_time, in_start_time) AS start_time,
                LEAST(lead(start_time) OVER (ORDER BY layer_id ASC), in_stop_time) AS stop_time
            FROM (
                SELECT * FROM {schema_name}.layer_ancestors(active_layer_id)
            ) as ancestors;
    END
$$ LANGUAGE plpgsql;