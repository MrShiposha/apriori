use crate::{
    graphics,
    object,
    r#type::{SessionId, LayerId, ObjectId, ObjectName, IntoStorageDuration},
    map_err,
    query,
    Result,
};
use postgres::Transaction;

pub struct Object<'t, 'storage> {
    transaction: &'t mut Transaction<'storage>,
}

impl<'t, 'storage> Object<'t, 'storage> {
    pub fn new_api(transaction: &'t mut Transaction<'storage>) -> Self {
        Self { transaction }
    }

    pub fn add(
        &mut self,
        session_id: SessionId,
        layer_id: LayerId,
        object: object::Object,
    ) -> Result<ObjectId> {
        self.transaction
            .query_one(
                query! {"
                    SELECT {schema_name}.add_object(
                        $1,
                        $2,
                        $3,
                        $4,
                        $5,
                        $6,
                        $7
                    )
                "},
                &[
                    &session_id,
                    &layer_id,
                    object.name(),
                    &object.radius(),
                    &graphics::pack_color(object.color()),
                    &object.mass(),
                    &object.compute_step().into_storage_duration(),
                ],
            )
            .map(|row| row.get(0))
            .map_err(map_err!(Error::Storage::Object))
    }

    pub fn is_object_exists(&mut self, session_id: SessionId, object_name: &ObjectName) -> Result<bool> {
        self.transaction
            .query_one(
                query!["SELECT {schema_name}.is_object_exists($1, $2)"],
                &[&session_id, object_name]
            )
            .map(|row| row.get(0))
            .map_err(map_err!(Error::Storage::Object))
    }

    // pub fn get_last_object_id(&mut self, session_id: SessionId) -> Result<ObjectId> {
    //     self.transaction
    //         .query_one(
    //             query!["SELECT {schema_name}.last_object_id($1)"],
    //             &[&session_id]
    //         )
    //         .map(|row| row.get(0))
    //         .map_err(map_err!(Error::Storage::Object))
    // }

    // pub fn rename(&mut self, session_id: SessionId, object_id: ObjectId, new_name: &str) -> Result<()> {
    //     self.manager
    //         .pool
    //         .get()?
    //         .execute(
    //             query!["CALL {schema_name}.rename_object($1, $2, $3)"],
    //             &[&session_id, &object_id, &new_name],
    //         ).map(|_| {})
    //         .map_err(map_err!(Error::Storage::RenameObject))
    // }

    // pub fn print_list(&mut self, session_id: SessionId) -> Result<()> {
    //     let set = self
    //         .manager
    //         .pool
    //         .get()?
    //         .query(
    //             query! {"
    //                 SELECT object_name
    //                 FROM {schema_name}.object
    //                 WHERE session_fk_id = $1
    //                 ORDER BY object_name
    //             "},
    //             &[&session_id]
    //         ).map_err(map_err![Error::Storage::ObjectList])?;

    //     for row in set {
    //         let name: &str = row.get(0);
    //         println!("\t{}", name);
    //     }

    //     Ok(())
    // }
}
