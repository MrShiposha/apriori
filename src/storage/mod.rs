use super::{app, make_error, Result};

pub mod session;
pub mod object;
pub mod attractor;

pub use object::Object;
pub use attractor::Attractor;
pub use session::Session;

#[macro_export]
macro_rules! storage_map_err {
    ($($err:ident)::+) => {
        |err| $crate::make_error![$($err)::+(err)]
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

    pub(in crate::storage) add_object: postgres::Statement,
    pub(in crate::storage) rename_object: postgres::Statement,
    pub(in crate::storage) object_list: postgres::Statement,
    pub(in crate::storage) add_attractor: postgres::Statement,
}

impl StorageManager {
    pub fn setup(connection_string: &str) -> Result<Self> {
        let mut psql = postgres::Client::connect(connection_string, postgres::NoTls)?;

        macro_rules! query {
            ($query:expr $(, $($additional:tt)*)?) => {
                psql.prepare(
                    format!(
                        $query,
                        schema_name = $crate::app::APP_NAME
                        $(, $($additional)*)?
                    ).as_str()
                )
            };
        }

        StorageManager::setup_schema(&mut psql)?;

        let create_new_session = query!["SELECT {schema_name}.create_new_session($1)"]?;

        let update_session_access_time =
            query!["CALL {schema_name}.update_session_access_time($1)"]?;

        let unlock_session = query!["CALL {schema_name}.unlock_session($1)"]?;

        let save_session = query!["CALL {schema_name}.save_session($1, $2)"]?;

        let rename_session = query!["CALL {schema_name}.rename_session($1, $2)"]?;

        let load_session = query!["SELECT {schema_name}.load_session($1)"]?;

        let session_list = query! {"
            SELECT session_name, last_access, is_locked 
            FROM {schema_name}.session
            WHERE session_name IS NOT NULL
            ORDER BY session_name
        "}?;

        let get_session = query!["SELECT {schema_name}.get_session($1)"]?;

        let delete_session = query!["CALL {schema_name}.delete_session($1)"]?;

        let add_object = query! {"
            SELECT {schema_name}.add_object(
                $1,
                $2,
                $3,
                $4,
                $5,
                $6
            )
        "}?;

        let rename_object = query!["CALL {schema_name}.rename_object($1, $2, $3)"]?;

        let add_attractor = query!["
            SELECT {schema_name}.add_attractor(
                $1, 
                $2, 
                $3, 
                $4,
                $5,
                $6,
                $7
            )
        "]?;

        let object_list = query! {"
            SELECT object_name
            FROM {schema_name}.object
            WHERE session_fk_id = $1
            ORDER BY object_name
        "}?;

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

            add_object,
            rename_object,
            object_list,
            add_attractor,
        };

        Ok(mgr)
    }

    fn setup_schema(psql: &mut postgres::Client) -> Result<()> {
        macro_rules! query {
            ($query:expr $(, $($additional:tt)*)?) => {
                format!(
                    $query,
                    schema_name = $crate::app::APP_NAME
                    $(, $($additional)*)?
                ).as_str()
            };
        }

        let setup_query = format! {
            r#"
                {schema} 
                {session} 
                {object}
                {location}
                {attractor}
                {session_triggers}
            "#,
            schema = query![include_str!("sql/setup/schema.sql")],
            session = query! {
                include_str!("sql/setup/session.sql"),
                session_max_hang_time = app::SESSION_MAX_HANG_TIME.num_seconds()
            },
            object = query![include_str!["sql/setup/object.sql"]],
            location = query![include_str!["sql/setup/location.sql"]],
            attractor = query![include_str!["sql/setup/attractor.sql"]],
            session_triggers = query![include_str!("sql/setup/session_triggers.sql")]
        };

        psql.batch_execute(setup_query.as_str())
            .map_err(|err| make_error![Error::Storage::SetupSchema(err)])
    }

    pub fn session(&mut self) -> Session {
        Session::new_api(self)
    }

    pub fn object(&mut self) -> Object {
        Object::new_api(self)
    }

    pub fn attractor(&mut self) -> Attractor {
        Attractor::new_api(self)
    }
}
