use crate::{
    query,
    object::GenCoord,
    r#type::{AsAbsoluteTime, IntoStorageDuration, LayerId, ObjectId},
    map_err, Result,
};
use postgres::Transaction;

pub struct Location<'t, 'storage> {
    transaction: &'t mut Transaction<'storage>,
}

impl<'t, 'storage> Location<'t, 'storage> {
    pub fn new_api(transaction: &'t mut Transaction<'storage>) -> Self {
        Self { transaction }
    }

    pub fn add(&mut self, object_id: ObjectId, layer_id: LayerId, coord: GenCoord) -> Result<()> {
        let location = coord.location();
        let velocity = coord.velocity();

        self.transaction
            .execute(
                query!["CALL {schema_name}.add_location($1, $2, $3, $4, $5, $6, $7, $8, $9)"],
                &[
                    &object_id,
                    &layer_id,
                    &coord.time().as_absolute_time().into_storage_duration(),
                    &location[0],
                    &location[1],
                    &location[2],
                    &velocity[0],
                    &velocity[1],
                    &velocity[2],
                ]
            )
            .map(|_| {})
            .map_err(map_err!(Error::Storage::Location))
    }
}