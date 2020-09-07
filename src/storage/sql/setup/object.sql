CREATE TABLE IF NOT EXISTS {schema_name}.object
(
    object_id bigserial PRIMARY KEY,
    session_fk_id serial 
        NOT NULL
        REFERENCES {schema_name}.session(session_id)
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
            object_name,
            radius,
            color,
            mass,
            compute_step
        ) RETURNING object_id INTO new_object_id;

       RETURN (new_object_id);
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