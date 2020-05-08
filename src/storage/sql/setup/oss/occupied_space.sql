CREATE VIRTUAL TABLE occupied_space USING rtree(
    id,
    x_min, x_max,
    y_min, y_max,
    z_min, z_max,
    t_min, t_max,
    +object_id,
    +bvx, +bvy, +bvz, -- begin velocity [xyz]
    +evx, +evy, +evz, --   end velocity [xyz]
    +cube_size,
    +location_info
);