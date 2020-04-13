use super::{
    Result,
    Error,
    app
};

pub mod session;
use session::Session;

#[macro_export]
macro_rules! schema_query {
    ($query:expr $(, $($additional:tt)*)?) => {
        format!(
            $query, 
            schema_name = $crate::app::APP_NAME
            $(, $($additional)*)?
        ).as_str()
    };
}

pub struct StorageManager {
    pub(in crate::storage) psql: postgres::Client,
    pub(in crate::storage) create_new_session: postgres::Statement,
    pub(in crate::storage) update_session_access_time: postgres::Statement,
    pub(in crate::storage) unlock_session: postgres::Statement,
    pub(in crate::storage) save_session: postgres::Statement,
    pub(in crate::storage) rename_session: postgres::Statement,
    pub(in crate::storage) load_session: postgres::Statement,
    pub(in crate::storage) session_list: postgres::Statement,
    pub(in crate::storage) get_session: postgres::Statement,
    pub(in crate::storage) delete_session: postgres::Statement,
}

impl StorageManager {
    pub fn setup(connection_string: &str) -> Result<Self> {
        let mut psql = postgres::Client::connect(connection_string, postgres::NoTls)?;

        StorageManager::setup_schema(&mut psql)?;

        let create_new_session = psql.prepare(
            schema_query!["SELECT {schema_name}.create_new_session($1)"]
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

    fn setup_schema(psql: &mut postgres::Client) -> Result<()> {
        let setup_query = format! {
            r#"
                {schema} 
                {session} 
                {session_triggers}
            "#,
            schema = schema_query![include_str!("sql/setup/schema.sql")],
            session = schema_query! {
                include_str!("sql/setup/session.sql"),
                session_max_hang_time = app::SESSION_MAX_HANG_TIME.num_seconds()
            },
            session_triggers = schema_query![include_str!("sql/setup/session_triggers.sql")]
        };

        psql.batch_execute(setup_query.as_str())
            .map_err(|err| Error::SetupSchema(err))
    }

    pub fn session<'storage>(&'storage mut self) -> Session<'storage> {
        Session::new_api(self)
    }
}