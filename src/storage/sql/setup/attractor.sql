CREATE TABLE IF NOT EXISTS {schema_name}.attractor
(
    attractor_id bigserial PRIMARY KEY,
    session_fk_id serial 
        NOT NULL
        REFERENCES {schema_name}.session(session_id)
        ON DELETE CASCADE,
    attractor_name varchar(50) NOT NULL,
    mass real NOT NULL,
    gravity real NOT NULL,
    location_x real NOT NULL,
    location_y real NOT NULL,
    location_z real NOT NULL,
    
    UNIQUE (session_fk_id, attractor_name)
);

CREATE OR REPLACE FUNCTION {schema_name}.add_attractor(
    session_id integer,
    attractor_name varchar(50),
    mass real,
    gravity real,
    location_x real,
    location_y real,
    location_z real
) RETURNS integer 
AS $$
    DECLARE
        new_attractor_id integer;
    BEGIN
        INSERT INTO {schema_name}.attractor
        VALUES(
            DEFAULT,
            session_id,
            attractor_name,
            mass,
            gravity,
            location_x,
            location_y,
            location_z
        ) RETURNING attractor_id INTO new_attractor_id;

       RETURN (new_attractor_id);
    END
$$ LANGUAGE plpgsql;