CREATE TABLE IF NOT EXISTS {schema_name}.location
(
    location_id bigserial PRIMARY KEY,
    object_fk_id bigserial
        NOT NULL
        REFERENCES {schema_name}.object
        ON DELETE CASCADE,
    x real NOT NULL,
    y real NOT NULL,
    z real NOT NULL,
    t bigint NOT NULL,
    vx real NOT NULL,
    vy real NOT NULL,
    vz real NOT NULL
);