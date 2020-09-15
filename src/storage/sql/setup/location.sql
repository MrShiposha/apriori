CREATE TABLE IF NOT EXISTS {schema_name}.location
(
    location_id bigserial PRIMARY KEY,
    object_fk_id bigserial
        NOT NULL
        REFERENCES {schema_name}.object
        ON DELETE CASCADE,
    layer_fk_id serial
        NOT NULL
        REFERENCES {schema_name}.layer
        ON DELETE CASCADE,
    t bigint NOT NULL,
    x real NOT NULL,
    y real NOT NULL,
    z real NOT NULL,
    vx real NOT NULL,
    vy real NOT NULL,
    vz real NOT NULL,

    vcx real NULL, -- vx after collision
    vcy real NULL, -- vy after collision
    vcz real NULL  -- vz after collision
);

CREATE TABLE IF NOT EXISTS {schema_name}.collision_partners
(
    location_fk_id bigserial
        NOT NULL
        REFERENCES {schema_name}.location
        ON DELETE CASCADE,
    partner_fk_id bigserial
        NOT NULL
        REFERENCES {schema_name}.location
        ON DELETE CASCADE
);

CREATE OR REPLACE PROCEDURE {schema_name}.add_location(
    object_id bigint,
    layer_id integer,
    t bigint,
    x real,
    y real,
    z real,
    vx real,
    vy real,
    vz real
) AS $$
    BEGIN
        INSERT INTO {schema_name}.location VALUES
        (DEFAULT, object_id, layer_id, t, x, y, z, vx, vy, vz, NULL);
    END
$$ LANGUAGE plpgsql;

-- CREATE OR REPLACE FUNCTION {schema_name}.is_objects_computed_at(
--     active_layer_id integer,
--     in_start_time bigint
-- ) RETURNS boolean
-- AS $$
--     BEGIN
--         RETURN (
--             WITH active_objects AS (
--                 SELECT object_id
--                 FROM {schema_name}.object
--                 INNER JOIN {schema_name}.layer_ancestors(active_layer_id) ancestors
--                     ON layer_fk_id = ancestors.layer_id
--             ) SELECT MIN(max_obj_time) >= in_start_time FROM (
--                 SELECT MAX(t) as max_obj_time
--                 FROM {schema_name}.location
--                 INNER JOIN active_objects o
--                     ON object_fk_id = o.object_id
--                 GROUP BY object_fk_id
--             ) AS is_min_obj_computed
--         );
--     END
-- $$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION {schema_name}.min_valid_start_time(
    active_layer_id integer,
    requested_time bigint
) RETURNS bigint
AS $$
    BEGIN
        RETURN (
            WITH active_objects AS (
                SELECT object_id
                FROM {schema_name}.object
                INNER JOIN {schema_name}.layer_ancestors(active_layer_id) ancestors
                    ON layer_fk_id = ancestors.layer_id
            ) SELECT COALESCE(MIN(max_obj_time), requested_time) FROM (
                SELECT MAX(t) as max_obj_time
                FROM {schema_name}.location
                INNER JOIN active_objects o
                    ON object_fk_id = o.object_id
                GROUP BY object_fk_id
            ) AS is_min_obj_computed
        );
    END
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION {schema_name}.query_object_layers_info(
    active_layer_id integer,
    object_id bigint,
    in_start_time bigint,
    in_stop_time bigint
) RETURNS TABLE (
    layer_id integer,
    layer_start_time bigint,
    layer_stop_time bigint
) AS $$
    DECLARE
        obj_compute_step bigint;
    BEGIN
        SELECT 2*compute_step
        FROM {schema_name}.object
        INTO obj_compute_step;

        RETURN QUERY
        SELECT
            temp_layer_id as layer_id,
            GREATEST(temp_start_time, in_start_time - obj_compute_step) AS start_time,
            LEAST(lead(temp_start_time) OVER (ORDER BY temp_layer_id ASC), in_stop_time + obj_compute_step) AS stop_time
        FROM (
            SELECT
                layer_fk_id as temp_layer_id, MIN(t) as temp_start_time
            FROM {schema_name}.location
            INNER JOIN {schema_name}.layer_ancestors(active_layer_id) ancestors
                ON layer_fk_id = ancestors.layer_id
            WHERE object_fk_id = object_id
            GROUP BY layer_fk_id
        ) as start_times;
    END
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION {schema_name}.range_locations(
    active_layer_id integer,
    in_start_time bigint,
    in_stop_time bigint
) RETURNS TABLE (
    out_location_id bigint,
    out_object_fk_id bigint,
    out_t bigint,
    out_x real,
    out_y real,
    out_z real,
    out_vx real,
    out_vy real,
    out_vz real,

    out_vcx real, -- vx after collision
    out_vcy real, -- vy after collision
    out_vcz real, -- vz after collision

    out_collision_partners bigint[]
)
AS $$
    BEGIN
        RETURN QUERY
        SELECT DISTINCT ON (object_fk_id, t)
            location_id,
            object_fk_id,
            t, x, y, z, vx, vy, vz, vcx, vcy, vcz,
            COALESCE(c_partners.partners_array, '{{}}')
        FROM {schema_name}.location
        -- INNER JOIN {schema_name}.query_layers_info(active_layer_id, in_start_time, in_stop_time) layers_info
        INNER JOIN {schema_name}.query_object_layers_info(active_layer_id, object_fk_id, in_start_time, in_stop_time) layers_info
            ON layer_fk_id = layers_info.layer_id
            AND t BETWEEN layers_info.layer_start_time AND layers_info.layer_stop_time
        LEFT OUTER JOIN (
            SELECT location_fk_id, array_agg(partner_fk_id) AS partners_array
            FROM {schema_name}.collision_partners
            GROUP BY location_fk_id
        ) c_partners
            ON c_partners.location_fk_id = location_id
        ORDER BY object_fk_id, t, location_id DESC;
    END
$$ LANGUAGE plpgsql;