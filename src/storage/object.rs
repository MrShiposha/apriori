use crate::{
    graphics,
    scene::Object4d,
    r#type::{SessionId, ObjectId},
    storage_map_err, Result,
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
            .psql
            .query_one(
                &self.manager.add_object,
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

    pub fn rename(&mut self, session_id: SessionId, old_name: &str, new_name: &str) -> Result<()> {
        self.manager
            .psql
            .execute(
                &self.manager.rename_object,
                &[&session_id, &old_name, &new_name],
            )
            .map(|_| {})
            .map_err(storage_map_err!(Error::Storage::RenameObject))
    }

    pub fn print_list(&mut self, session_id: &SessionId) -> Result<()> {
        let set = self
            .manager
            .psql
            .query(&self.manager.object_list, &[session_id])
            .map_err(storage_map_err![Error::Storage::ObjectList])?;

        for row in set {
            let name: &str = row.get(0);
            println!("\t{}", name);
        }

        Ok(())
    }
}
