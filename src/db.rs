use postgres::{self as psql, NoTls};
use super::{
    Result,
    Error,
    app,
    r#type::SessionId
};

macro_rules! schema_query {
    ($query:expr $(, $($additional:tt)*)?) => {
        format!(
            $query, 
            schema_name = app::APP_NAME
            $(, $($additional)*)?
        ).as_str()
    };
}

pub struct StorageManager {
    psql: postgres::Client,
    create_new_session: postgres::Statement,
    update_session_access_time: postgres::Statement,
    unlock_session: postgres::Statement,
    save_session: postgres::Statement,
    rename_session: postgres::Statement,
    load_session: postgres::Statement,
    session_list: postgres::Statement,
    get_session: postgres::Statement,
    delete_session: postgres::Statement,
}

impl StorageManager {
    pub fn connect(connection_string: &str) -> Result<Self> {
        let mut psql = postgres::Client::connect(connection_string, NoTls)?;

        StorageManager::setup_schema(&mut psql)?;

        let create_new_session = psql.prepare(
            schema_query!["SELECT {schema_name}.create_new_session()"]
        )?;

        let update_session_access_time = psql.prepare(
           schema_query!["CALL {schema_name}.update_session_access_time($1)"] 
        )?;

        let unlock_session = psql.prepare(
            schema_query!["CALL {schema_name}.unlock_session($1)"]
        )?;

        let save_session = psql.prepare(
            schema_query!["CALL {schema_name}.save_session($1, $2)"]
        )?;

        let rename_session = psql.prepare(
            schema_query!["CALL {schema_name}.rename_session($1, $2)"]
        )?;

        let load_session = psql.prepare(
            schema_query!["SELECT {schema_name}.load_session($1)"]
        )?;

        let session_list = psql.prepare(
            schema_query! {r#"
                SELECT session_name, last_access 
                FROM {schema_name}.session
                WHERE session_name IS NOT NULL
                ORDER BY last_access
            "#}
        )?;

        let get_session = psql.prepare(
            schema_query!["SELECT {schema_name}.get_session($1)"]
        )?;

        let delete_session = psql.prepare(
            schema_query!["CALL {schema_name}.delete_session($1)"]
        )?;

        let mgr = Self {
            psql,
            create_new_session,
            update_session_access_time,
            unlock_session,
            save_session,
            rename_session,
            load_session,
            session_list,
            get_session,
            delete_session,
        };

        Ok(mgr)
    }

    fn setup_schema(client: &mut postgres::Client) -> Result<()> {
        let setup_query = format! {
            r#"
                {schema} 
                {session} 
                {session_triggers}
            "#,
            schema = schema_query![include_str!("sql/setup/schema.sql")],
            session = schema_query![include_str!("sql/setup/session.sql")],
            session_triggers = schema_query! {
                include_str!("sql/setup/session_triggers.sql"),
                session_max_hang_time = app::SESSION_MAX_HANG_TIME.num_seconds()
            }
        };

        client.batch_execute(setup_query.as_str())
            .map_err(|err| Error::SetupSchema(err))
    }

    pub fn create_new_session(&mut self) -> Result<SessionId> {
        let row = self.psql.query_one(&self.create_new_session, &[])
            .map_err(|err| Error::SessionCreate(err))?;

        Ok(row.get(0))
    }

    pub fn update_session_access_time(&mut self, id: SessionId) -> Result<()> {
        self.psql.execute(&self.update_session_access_time, &[&id])
            .map_err(|err| Error::SessionUpdateAccessTime(err))?;

        Ok(())
    }

    pub fn unlock_session(&mut self, id: SessionId) -> Result<()> {
        self.psql.execute(&self.unlock_session, &[&id])
            .map(|_| {})
            .map_err(|err| Error::SessionUnlock(err))
    }

    pub fn save_session(&mut self, id: SessionId, name: &str) -> Result<()> {
        match self.psql.execute(&self.save_session, &[&id, &name]) {
            Ok(_) => Ok(()),
            Err(err) => match err.code() {
                Some(code) if *code == psql::error::SqlState::UNIQUE_VIOLATION => {
                    Err(Error::SessionSave(
                        format!("session with name `{}` already exists", name)
                    ))
                },
                _ => Err(Error::SessionSave(err.to_string()))
            }       
        }
    }

    pub fn load_session(&mut self, name: &str) -> Result<SessionId> {
        let row = self.psql.query_one(&self.load_session, &[&name])
            .map_err(|err| Error::SessionLoad(err))?;

        Ok(row.get(0))
    }

    pub fn rename_session(&mut self, old_name: &str, new_name: &str) -> Result<()> {
        self.psql.execute(&self.rename_session, &[&old_name, &new_name])
            .map(|_| {})
            .map_err(|err| Error::SessionRename(err))
    }

    pub fn list_sessions(&mut self) -> Result<()> {
        let set = self.psql.query(&self.session_list, &[])
            .map_err(|err| Error::SessionList(err))?;

        for row in set {
            let name: &str = row.get(0);
            let last_access: chrono::DateTime<chrono::Local> = row.get(1);
            println!("\t{} [last access {}]", name, last_access);
        }

        Ok(())
    }

    pub fn get_session(&mut self, id: SessionId) -> Result<()> {
        let row = self.psql.query_one(&self.get_session, &[&id])
            .map_err(|err| Error::SessionGet(err))?;

        let name: Option<_> = row.get(0);
        println!("{}", name.unwrap_or("/unnamed/"));
        Ok(())
    }

    pub fn delete_session(&mut self, name: &str) -> Result<()> {
        self.psql.execute(&self.delete_session, &[&name])
            .map(|_| {})
            .map_err(|err| Error::SessionDelete(err))
    }
}