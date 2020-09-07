use crate::{
    make_error, query,
    r#type::{IntoRustDuration, IntoStorageDuration, LayerId, LayerName, RawTime, SessionId},
    storage_map_err, Result,
};
use postgres::Transaction;

pub struct Layer<'t, 'storage> {
    transaction: &'t mut Transaction<'storage>,
}

impl<'t, 'storage> Layer<'t, 'storage> {
    pub fn new_api(transaction: &'t mut Transaction<'storage>) -> Self {
        Self { transaction }
    }

    pub fn get_name(&mut self, layer_id: LayerId) -> Result<LayerName> {
        self.transaction
            .query_one(query!["SELECT {schema_name}.layer_name($1);"], &[&layer_id])
            .map(|row| row.get(0))
            .map_err(storage_map_err!(Error::Storage::Layer))
    }

    pub fn get_start_time(&mut self, layer_id: LayerId) -> Result<chrono::Duration> {
        self.transaction
            .query_one(
                query!["SELECT {schema_name}.layer_start_time($1)"],
                &[&layer_id],
            )
            .map(|row| {
                let raw_time: RawTime = row.get(0);

                raw_time.into_rust_duration()
            })
            .map_err(storage_map_err!(Error::Storage::Layer))
    }

    pub fn rename_layer(&mut self, layer_id: LayerId, new_layer_name: &LayerName) -> Result<()> {
        self.transaction
            .execute(
                query!["CALL {schema_name}.rename_layer($1, $2)"],
                &[&layer_id, &new_layer_name],
            )
            .map(|_| {})
            .map_err(storage_map_err!(Error::Storage::Layer))
    }

    pub fn get_layer_id(
        &mut self,
        session_id: SessionId,
        layer_name: &LayerName,
    ) -> Result<LayerId> {
        let row = self
            .transaction
            .query_one(
                query!["SELECT {schema_name}.layer_id($1, $2)"],
                &[&session_id, layer_name],
            )
            .map_err(storage_map_err!(Error::Storage::Layer))?;

        row.try_get(0)
            .map_err(|_| make_error![Error::Layer::LayerNotFound(layer_name.clone())])
    }

    pub fn get_main_layer(&mut self, session_id: SessionId) -> Result<LayerId> {
        self.transaction
            .query_one(
                query!["SELECT {schema_name}.main_layer_id($1)"],
                &[&session_id],
            )
            .map(|row| row.get(0))
            .map_err(storage_map_err!(Error::Storage::Layer))
    }

    pub fn get_layer_children(
        &mut self,
        session_id: SessionId,
        layer_id: LayerId,
    ) -> Result<Vec<LayerId>> {
        self.transaction
            .query_one(
                query!["SELECT {schema_name}.layer_children($1, $2)"],
                &[&session_id, &layer_id],
            )
            .map(|row| row.try_get(0).unwrap_or(vec![]))
            .map_err(storage_map_err!(Error::Storage::Layer))
    }

    pub fn get_current_layer_id(
        &mut self,
        active_layer_id: LayerId,
        vtime: chrono::Duration,
    ) -> Result<LayerId> {
        self.transaction
            .query_one(
                query!["SELECT {schema_name}.current_layer_id($1, $2)"],
                &[&active_layer_id, &vtime.into_storage_duration()],
            )
            .map(|row| row.get(0))
            .map_err(storage_map_err!(Error::Storage::Layer))
    }

    pub fn add_layer(
        &mut self,
        session_id: SessionId,
        active_layer_id: LayerId,
        new_layer_name: &LayerName,
        new_layer_start_time: chrono::Duration,
    ) -> Result<LayerId> {
        self.transaction
            .query_one(
                query! {"
                    SELECT {schema_name}.add_layer(
                        $1,
                        $2,
                        $3,
                        $4
                    )
                "},
                &[
                    &session_id,
                    &active_layer_id,
                    &new_layer_name,
                    &new_layer_start_time.into_storage_duration(),
                ],
            )
            .map(|row| row.get(0))
            .map_err(storage_map_err!(Error::Storage::Layer))
    }

    pub fn layer_ancestors(&mut self, layer_id: LayerId) -> Result<Vec<LayerId>> {
        let rows = self
            .transaction
            .query(
                query!["SELECT layer_id FROM {schema_name}.layer_ancestors($1)"],
                &[&layer_id],
            )
            .map_err(storage_map_err!(Error::Storage::Layer))?;

        Ok(rows.into_iter().map(|row| row.get(0)).collect())
    }

    pub fn remove_layer(&mut self, layer_id: LayerId) -> Result<()> {
        self.transaction
            .execute(query!["CALL {schema_name}.remove_layer($1)"], &[&layer_id])
            .map(|_| {})
            .map_err(storage_map_err!(Error::Storage::Layer))
    }
}
