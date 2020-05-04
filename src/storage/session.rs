use crate::{r#type::{SessionId, SessionName}, storage_map_err, Result};

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
            .psql
            .query_one(&self.manager.create_new_session, &[&name])
            .map_err(storage_map_err!(Error::Storage::SessionCreate))?;

        Ok(row.get(0))
    }

    pub fn update_access_time(&mut self, id: SessionId) -> Result<()> {
        self.manager
            .psql
            .execute(&self.manager.update_session_access_time, &[&id])
            .map_err(storage_map_err![Error::Storage::SessionUpdateAccessTime])?;

        Ok(())
    }

    pub fn unlock(&mut self, id: SessionId) -> Result<()> {
        self.manager
            .psql
            .execute(&self.manager.unlock_session, &[&id])
            .map(|_| {})
            .map_err(storage_map_err![Error::Storage::SessionUnlock])
    }

    pub fn save(&mut self, id: SessionId, name: &str) -> Result<()> {
        self.manager
            .psql
            .execute(&self.manager.save_session, &[&id, &name])
            .map(|_| {})
            .map_err(storage_map_err!(Error::Storage::SessionSave))
    }

    pub fn load(&mut self, name: &str) -> Result<SessionId> {
        let row = self
            .manager
            .psql
            .query_one(&self.manager.load_session, &[&name])
            .map_err(storage_map_err![Error::Storage::SessionLoad])?;

        Ok(row.get(0))
    }

    pub fn rename(&mut self, old_name: &str, new_name: &str) -> Result<()> {
        self.manager
            .psql
            .execute(&self.manager.rename_session, &[&old_name, &new_name])
            .map(|_| {})
            .map_err(storage_map_err!(Error::Storage::SessionRename))
    }

    pub fn print_list(&mut self) -> Result<()> {
        let set = self
            .manager
            .psql
            .query(&self.manager.session_list, &[])
            .map_err(storage_map_err![Error::Storage::SessionList])?;

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
            .psql
            .query_one(&self.manager.get_session, &[&id])
            .map_err(storage_map_err![Error::Storage::SessionGet])?;

        let name: Option<_> = row.get(0);
        println!("{}", name.unwrap_or("/unnamed/"));
        Ok(())
    }

    pub fn delete(&mut self, name: &str) -> Result<()> {
        self.manager
            .psql
            .execute(&self.manager.delete_session, &[&name])
            .map(|_| {})
            .map_err(storage_map_err![Error::Storage::SessionDelete])
    }
}
