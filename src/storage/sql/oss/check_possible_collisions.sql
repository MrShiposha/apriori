SELECT +object_id,
    x_min, x_max,
    y_min, y_max,
    z_min, z_max,
    t_min, t_max,
    +bvx, +bvy, +bvz,
    +evx, +evy, +evz,
    +cube_size,
    +location_info
FROM
    occupied_space
WHERE
        x_max > ?1 AND x_min < ?2
    AND y_max > ?3 AND y_min < ?4
    AND z_max > ?5 AND z_min < ?6
    AND t_max > ?7 AND t_min < ?8
    AND +object_id != ?9;