use crate::{
    graphics,
    message::AddObject,
    r#type::{ObjectId, ObjectName, SessionId},
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
        add_msg: &AddObject,
        default_name: &ObjectName,
    ) -> Result<ObjectId> {
        let name = add_msg.name.as_ref().unwrap_or(default_name);

        let color = add_msg.color.unwrap_or(graphics::random_color());

        self.manager
            .psql
            .query_one(
                &self.manager.add_object,
                &[
                    &session_id,
                    name,
                    &add_msg.radius,
                    &graphics::pack_color(&color),
                    &add_msg.mass,
                    &add_msg.step.num_milliseconds(),
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
