CREATE VIRTUAL TABLE occupied_space USING rtree(
    id,
    x_min, x_max,
    y_min, y_max,
    z_min, z_max,
    t_min, t_max,
    +object_id BIGINT
);