CREATE TABLE IF NOT EXISTS {schema_name}.object
(
    object_id bigserial PRIMARY KEY,
    session_fk_id serial 
        NOT NULL
        REFERENCES {schema_name}.session(session_id)
        ON DELETE CASCADE,
    object_name varchar(50) UNIQUE NOT NULL,
    radius real NOT NULL,
    color integer NOT NULL,
    mass real NOT NULL,
    gravity_coeff real NOT NULL,
    compute_step bigint NOT NULL,
    time_border_future bigint,
    time_border_past bigint
);

CREATE OR REPLACE PROCEDURE {schema_name}.add_object(
    session_id integer,
    object_name varchar(50),
    radius real,
    color integer,
    mass real,
    gravity_coeff real,
    compute_step bigint,
    time_border_future bigint,
    time_border_past bigint
) AS $$
    BEGIN
       INSERT INTO {schema_name}.object
       VALUES(
           DEFAULT,
           session_id,
           object_name,
           radius,
           color,
           mass,
           gravity_coeff,
           compute_step,
           time_border_future,
           time_border_past
        );
    END
$$ LANGUAGE plpgsql;

CREATE OR REPLACE PROCEDURE {schema_name}.rename_object(
    session_id integer,
    old_name varchar(50), 
    new_name varchar(50)
) AS $$
    BEGIN
        UPDATE {schema_name}.object
        SET object_name = new_name
        WHERE session_fk_id = session_id AND object_name = old_name;

        IF (NOT FOUND) THEN
            RAISE 'object `%` not found', old_name;
        END IF;
    END
$$ LANGUAGE plpgsql;