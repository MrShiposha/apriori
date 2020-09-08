use crate::{
    query,
    r#type::{LayerId, SessionId, SessionInfo, SessionName},
    map_err, Result,
};
use postgres::Transaction;

pub struct Session<'t, 'storage> {
    transaction: &'t mut Transaction<'storage>,
}

impl<'t, 'storage> Session<'t, 'storage> {
    pub fn new_api(transaction: &'t mut Transaction<'storage>) -> Self {
        Self { transaction }
    }

    pub fn new(&mut self, name: Option<SessionName>) -> Result<(SessionId, LayerId)> {
        let row = self
            .transaction
            .query_one(
                query!["SELECT {schema_name}.create_new_session($1)"],
                &[&name],
            )
            .map_err(map_err!(Error::Storage::SessionCreate))?;

        let session_id: SessionId = row.get(0);
        let row = self
            .transaction
            .query_one(
                query!("SELECT {schema_name}.main_layer_id($1)"),
                &[&session_id],
            )
            .map_err(map_err!(Error::Storage::SessionCreate))?;

        let layer_id = row.get(0);

        Ok((session_id, layer_id))
    }

    pub fn update_access_time(&mut self, id: SessionId) -> Result<()> {
        self.transaction
            .execute(
                query!["CALL {schema_name}.update_session_access_time($1)"],
                &[&id],
            )
            .map_err(map_err![Error::Storage::SessionUpdateAccessTime])?;

        Ok(())
    }

    pub fn unlock(&mut self, id: SessionId) -> Result<()> {
        self.transaction
            .execute(query!["CALL {schema_name}.unlock_session($1)"], &[&id])
            .map(|_| {})
            .map_err(map_err![Error::Storage::SessionUnlock])
    }

    pub fn save(&mut self, id: SessionId, name: &str) -> Result<()> {
        self.transaction
            .execute(
                query!["CALL {schema_name}.save_session($1, $2)"],
                &[&id, &name],
            )
            .map(|_| {})
            .map_err(map_err!(Error::Storage::SessionSave))
    }

    pub fn load(&mut self, name: &str) -> Result<(SessionId, LayerId)> {
        let row = self
            .transaction
            .query_one(query!["SELECT {schema_name}.load_session($1)"], &[&name])
            .map_err(map_err![Error::Storage::SessionLoad])?;

        let session_id: SessionId = row.get(0);

        let row = self
            .transaction
            .query_one(
                query!("SELECT {schema_name}.main_layer_id($1)"),
                &[&session_id],
            )
            .map_err(map_err!(Error::Storage::SessionCreate))?;

        let layer_id: LayerId = row.get(0);

        Ok((session_id, layer_id))
    }

    pub fn rename(&mut self, old_name: &str, new_name: &str) -> Result<()> {
        self.transaction
            .execute(
                query!["CALL {schema_name}.rename_session($1, $2)"],
                &[&old_name, &new_name],
            )
            .map(|_| {})
            .map_err(map_err!(Error::Storage::SessionRename))
    }

    pub fn get_list(&mut self) -> Result<Vec<SessionInfo>> {
        let row = self
            .transaction
            .query(
                query! {"
                    SELECT session_name, last_access, is_locked
                    FROM {schema_name}.session
                    WHERE session_name IS NOT NULL
                    ORDER BY session_name
                "},
                &[],
            )
            .map_err(map_err![Error::Storage::SessionList])?;

        let infos = row
            .into_iter()
            .map(|row| SessionInfo {
                name: row.get(0),
                last_access: row.get(1),
                is_locked: row.get(2),
            })
            .collect();

        Ok(infos)
    }

    pub fn get_name(&mut self, id: SessionId) -> Result<SessionName> {
        self.transaction
            .query_one(query!["SELECT {schema_name}.get_session_name($1)"], &[&id])
            .map(|row| {
                let name: Option<_> = row.get(0);
                name.unwrap_or("/unnamed/".into())
            })
            .map_err(map_err![Error::Storage::SessionGet])
    }

    pub fn delete(&mut self, name: &str) -> Result<()> {
        self.transaction
            .execute(query!["CALL {schema_name}.delete_session($1)"], &[&name])
            .map(|_| {})
            .map_err(map_err![Error::Storage::SessionDelete])
    }
}
