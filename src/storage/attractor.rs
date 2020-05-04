use crate::{
    r#type::{SessionId, AttractorId, AttractorName},
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
        attractor: &crate::scene::Attractor,
        attractor_name: &AttractorName,
    ) -> Result<AttractorId> {
        let location = attractor.location();

        self.manager
            .psql
            .query_one(
                &self.manager.add_attractor,
                &[
                    &session_id,
                    attractor_name,
                    &attractor.mass(),
                    &attractor.gravity_coeff(),
                    &location[0],
                    &location[1],
                    &location[2],
                ],
            )
            .map(|row| row.get(0))
            .map_err(storage_map_err!(Error::Storage::AddObject))
    }
}