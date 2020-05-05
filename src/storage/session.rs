use crate::{r#type::{SessionId, SessionName}, storage_map_err, query, Result};

pub struct Session<'storage> {
    manager: &'storage mut super::StorageManager,
}

impl<'storage> Session<'storage> {
    pub fn new_api(manager: &'storage mut super::StorageManager) -> Self {
        Self { manager }
    }

    pub fn new(&mut self, name: Option<SessionName>) -> Result<SessionId> {
        let row = self
            .manager
            .pool
            .get()?
            .query_one(
                query!["SELECT {schema_name}.create_new_session($1)"],
                &[&name]
            ).map_err(storage_map_err!(Error::Storage::SessionCreate))?;

        Ok(row.get(0))
    }

    pub fn update_access_time(&mut self, id: SessionId) -> Result<()> {
        self.manager
            .pool
            .get()?
            .execute(
                query!["CALL {schema_name}.update_session_access_time($1)"], 
                &[&id]
            ).map_err(storage_map_err![Error::Storage::SessionUpdateAccessTime])?;

        Ok(())
    }

    pub fn unlock(&mut self, id: SessionId) -> Result<()> {
        self.manager
            .pool
            .get()?
            .execute(
                query!["CALL {schema_name}.unlock_session($1)"], 
                &[&id]
            ).map(|_| {})
            .map_err(storage_map_err![Error::Storage::SessionUnlock])
    }

    pub fn save(&mut self, id: SessionId, name: &str) -> Result<()> {
        self.manager
            .pool
            .get()?
            .execute(
                query!["CALL {schema_name}.save_session($1, $2)"], 
                &[&id, &name]
            ).map(|_| {})
            .map_err(storage_map_err!(Error::Storage::SessionSave))
    }

    pub fn load(&mut self, name: &str) -> Result<SessionId> {
        let row = self
            .manager
            .pool
            .get()?
            .query_one(
                query!["SELECT {schema_name}.load_session($1)"], 
                &[&name]
            ).map_err(storage_map_err![Error::Storage::SessionLoad])?;

        Ok(row.get(0))
    }

    pub fn rename(&mut self, old_name: &str, new_name: &str) -> Result<()> {
        self.manager
            .pool
            .get()?
            .execute(
                query!["CALL {schema_name}.rename_session($1, $2)"], 
                &[&old_name, &new_name]
            ).map(|_| {})
            .map_err(storage_map_err!(Error::Storage::SessionRename))
    }

    pub fn print_list(&mut self) -> Result<()> {
        let set = self
            .manager
            .pool
            .get()?
            .query(
                query! {"
                    SELECT session_name, last_access, is_locked 
                    FROM {schema_name}.session
                    WHERE session_name IS NOT NULL
                    ORDER BY session_name
                "}, 
                &[]
            ).map_err(storage_map_err![Error::Storage::SessionList])?;

        for row in set {
            let name: &str = row.get(0);
            let last_access: chrono::DateTime<chrono::Local> = row.get(1);
            let is_locked: bool = row.get(2);

            let locked_text = "LOCKED:";
            println!(
                "{is_locked:<width$} {} [last access {}]",
                name,
                last_access,
                is_locked = if is_locked { locked_text } else { "" },
                width = locked_text.len()
            );
        }

        Ok(())
    }

    pub fn print_current_name(&mut self, id: SessionId) -> Result<()> {
        let row = self
            .manager
            .pool
            .get()?
            .query_one(
                query!["SELECT {schema_name}.get_session_name($1)"], 
                &[&id]
            ).map_err(storage_map_err![Error::Storage::SessionGet])?;

        let name: Option<_> = row.get(0);
        println!("{}", name.unwrap_or("/unnamed/"));
        Ok(())
    }

    pub fn delete(&mut self, name: &str) -> Result<()> {
        self.manager
            .pool
            .get()?
            .execute(
                query!["CALL {schema_name}.delete_session($1)"], 
                &[&name]
            ).map(|_| {})
            .map_err(storage_map_err![Error::Storage::SessionDelete])
    }
}
