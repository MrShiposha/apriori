use crate::{
    message::AddAttractor,
    r#type::{SessionId, AttractorId},
    storage_map_err, Result,
};

pub struct Attractor<'storage> {
    manager: &'storage mut super::StorageManager,
}

impl<'storage> Attractor<'storage> {
    pub fn new_api(manager: &'storage mut super::StorageManager) -> Self {
        Self { manager }
    }

    pub fn add(
        &mut self,
        session_id: SessionId,
        add_msg: &AddAttractor,
        default_name: &String,
    ) -> Result<AttractorId> {
        let name = add_msg.name.as_ref().unwrap_or(default_name);

        let location = &add_msg.location;

        self.manager
            .psql
            .query_one(
                &self.manager.add_attractor,
                &[
                    &session_id,
                    name,
                    &add_msg.mass,
                    &add_msg.gravity_coeff,
                    &location[0],
                    &location[1],
                    &location[2],
                ],
            )
            .map(|row| row.get(0))
            .map_err(storage_map_err!(Error::Storage::AddObject))
    }
}