CREATE TABLE IF NOT EXISTS {schema_name}.location
(
    location_id bigserial PRIMARY KEY,
    object_fk_id bigserial
        NOT NULL
        REFERENCES {schema_name}.object
        ON DELETE CASCADE,
    t bigint NOT NULL,
    x real NOT NULL,
    y real NOT NULL,
    z real NOT NULL,
    vx real NOT NULL,
    vy real NOT NULL,
    vz real NOT NULL,

    canceled_location_fk_id bigint
        NULL
        REFERENCES {schema_name}.location
);

CREATE OR REPLACE PROCEDURE {schema_name}.add_location(
    object_id bigint,
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
        (DEFAULT, object_id, t, x, y, z, vx, vy, vz, NULL);
    END
$$ LANGUAGE plpgsql;