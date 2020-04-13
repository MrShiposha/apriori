use postgres::error::SqlState;
use crate::{
    Result,
    Error,
    r#type::SessionId
};

macro_rules! psql_err_code {
    (
        $err:path: $psql_err:expr => match code {
            $($sql_state_head:path => $err_expr_head:expr),+
        }
    ) => {
        match $psql_err.code() {
            $(
                Some(code) if *code == $sql_state_head => {
                    $err($err_expr_head)
                }
            )+
            _ => $err($psql_err.to_string())
        }
    };
}

pub struct Session<'storage> {
    manager: &'storage mut super::StorageManager
}

impl<'storage> Session<'storage> {
    pub fn new_api(manager: &'storage mut super::StorageManager) -> Self {
        Self {
            manager
        }
    }

    pub fn new(&mut self, name: Option<String>) -> Result<SessionId> {
        let row = self.manager.psql.query_one(&self.manager.create_new_session, &[&name])
            .map_err(|err| Error::SessionCreate(err))?;

        Ok(row.get(0))
    }

    pub fn update_access_time(&mut self, id: SessionId) -> Result<()> {
        self.manager.psql.execute(&self.manager.update_session_access_time, &[&id])
            .map_err(|err| Error::SessionUpdateAccessTime(err))?;

        Ok(())
    }

    pub fn unlock(&mut self, id: SessionId) -> Result<()> {
        self.manager.psql.execute(&self.manager.unlock_session, &[&id])
            .map(|_| {})
            .map_err(|err| Error::SessionUnlock(err))
    }

    pub fn save(&mut self, id: SessionId, name: &str) -> Result<()> {
        self.manager.psql.execute(&self.manager.save_session, &[&id, &name])
            .map(|_| {})
            .map_err(|err| psql_err_code![Error::SessionSave: err => match code {
                SqlState::UNIQUE_VIOLATION => format!("session with name `{}` already exists", name)
            }])
    }

    pub fn load(&mut self, name: &str) -> Result<SessionId> {
        let row = self.manager.psql.query_one(&self.manager.load_session, &[&name])
            .map_err(|err| Error::SessionLoad(err))?;

        Ok(row.get(0))
    }

    pub fn rename(&mut self, old_name: &str, new_name: &str) -> Result<()> {
        self.manager.psql.execute(&self.manager.rename_session, &[&old_name, &new_name])
            .map(|_| {})
            .map_err(|err| Error::SessionRename(err))
    }

    pub fn print_list(&mut self) -> Result<()> {
        let set = self.manager.psql.query(&self.manager.session_list, &[])
            .map_err(|err| Error::SessionList(err))?;

        for row in set {
            let name: &str = row.get(0);
            let last_access: chrono::DateTime<chrono::Local> = row.get(1);
            println!("\t{} [last access {}]", name, last_access);
        }

        Ok(())
    }

    pub fn get(&mut self, id: SessionId) -> Result<()> {
        let row = self.manager.psql.query_one(&self.manager.get_session, &[&id])
            .map_err(|err| Error::SessionGet(err))?;

        let name: Option<_> = row.get(0);
        println!("{}", name.unwrap_or("/unnamed/"));
        Ok(())
    }

    pub fn delete(&mut self, name: &str) -> Result<()> {
        self.manager.psql.execute(&self.manager.delete_session, &[&name])
            .map(|_| {})
            .map_err(|err| Error::SessionDelete(err))
    }
}