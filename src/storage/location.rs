use crate::{
    query,
    object::GenCoord,
    r#type::{RawTime, IntoStorageDuration, IntoRustDuration, LayerId, ObjectId},
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
                    &coord.time().into_storage_duration(),
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

    pub fn get_min_valid_start_time(
        &mut self,
        layer_id: LayerId,
        requested_time: chrono::Duration
    ) -> Result<chrono::Duration> {
        self.transaction
            .query_one(
                query!["SELECT {schema_name}.min_valid_start_time($1, $2)"],
                &[&layer_id, &requested_time.into_storage_duration()]
            )
            .map(|row| {
                let time: RawTime = row.get(0);

                time.into_rust_duration()
            })
            .map_err(map_err!(Error::Storage::Location))
    }
}