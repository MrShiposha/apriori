use crate::{
    graphics,
    r#type::{SessionId, ObjectId},
    storage_map_err,
    query,
    Result,
};

pub struct Object<'storage> {
    manager: &'storage mut super::StorageManager,
}

impl<'storage> Object<'storage> {
    pub fn new_api(manager: &'storage mut super::StorageManager) -> Self {
        Self { manager }
    }

    pub fn add(
        &mut self,
        session_id: SessionId,
        object: &Object4d,
    ) -> Result<ObjectId> {
        self.manager
            .pool
            .get()?
            .query_one(
                query! {"
                    SELECT {schema_name}.add_object(
                        $1,
                        $2,
                        $3,
                        $4,
                        $5,
                        $6
                    )
                "},
                &[
                    &session_id,
                    object.name(),
                    &object.radius(),
                    &graphics::pack_color(object.color()),
                    &object.mass(),
                    &object.track().compute_step().num_milliseconds(),
                ],
            )
            .map(|row| row.get(0))
            .map_err(storage_map_err!(Error::Storage::AddObject))
    }

    pub fn rename(&mut self, session_id: SessionId, object_id: ObjectId, new_name: &str) -> Result<()> {
        self.manager
            .pool
            .get()?
            .execute(
                query!["CALL {schema_name}.rename_object($1, $2, $3)"],
                &[&session_id, &object_id, &new_name],
            ).map(|_| {})
            .map_err(storage_map_err!(Error::Storage::RenameObject))
    }

    pub fn print_list(&mut self, session_id: SessionId) -> Result<()> {
        let set = self
            .manager
            .pool
            .get()?
            .query(
                query! {"
                    SELECT object_name
                    FROM {schema_name}.object
                    WHERE session_fk_id = $1
                    ORDER BY object_name
                "},
                &[&session_id]
            ).map_err(storage_map_err![Error::Storage::ObjectList])?;

        for row in set {
            let name: &str = row.get(0);
            println!("\t{}", name);
        }

        Ok(())
    }
}
