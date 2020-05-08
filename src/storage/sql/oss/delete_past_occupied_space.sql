DELETE FROM occupied_space
WHERE +object_id = ?1
    AND t_max < ?2;