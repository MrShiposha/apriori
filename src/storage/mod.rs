use {
    postgres::Transaction,
    r2d2_postgres::PostgresConnectionManager,
    crate::{
        make_error,
        Result,
        // r#type::{
        //     ObjectId,
        //     Coord,
        //     Distance,
        //     RelativeTime,
        //     Vector,
        // }
    }
};

pub mod session;
pub mod layer;
// pub mod object;

// pub use object::Object;
pub use session::Session;
pub use layer::Layer;

const LOG_TARGET: &'static str = "storage";

#[macro_export]
macro_rules! storage_map_err {
    ($($err:ident)::+) => {
        |err| $crate::make_error![$($err)::+(err)]
    };
}

#[macro_export]
macro_rules! query {
    ($query:expr $(, $($additional:tt)*)?) => {
        format!(
            $query,
            schema_name = $crate::app::APP_NAME
            $(, $($additional)*)?
        ).as_str()
    };
}

#[macro_export]
macro_rules! transaction {
    ($storage:expr => $trans:ident $($tt:tt)*) => {{
        let mut pooled_connection = $storage.pool.get()?;

        let mut $trans = pooled_connection
            .transaction()
            .map_err(|err| $crate::make_error![Error::Storage::Transaction(err)])?;

        $($tt)*

        $trans.commit()?;
    }};
}

#[derive(Clone)]
pub struct StorageManager {
    pub(in crate) pool: r2d2::Pool<PostgresConnectionManager<postgres::NoTls>>
}

impl StorageManager {
    pub fn setup(connection_string: &str, session_max_hang_time: chrono::Duration) -> Result<Self> {
        let mgr = PostgresConnectionManager::new(
            connection_string.parse()?,
            postgres::NoTls
        );

        let pool = r2d2::Pool::new(mgr)?;
        {
            let mut client = pool.get()?;
            Self::setup_schema(&mut client, session_max_hang_time)?;
        }

        let storage_mgr = Self {
            pool
        };

        Ok(storage_mgr)
    }

    fn setup_schema(psql: &mut postgres::Client, session_max_hang_time: chrono::Duration) -> Result<()> {
        let setup_query = format! {
            r#"
                {schema}
                {session}
                {layer}
                {object}
                {location}
                {session_triggers}
                {layer_triggers}
            "#,
            schema = query![include_str!("sql/setup/schema.sql")],
            session = query! {
                include_str!("sql/setup/session.sql"),
                session_max_hang_time = session_max_hang_time.num_seconds()
            },
            layer = query![include_str!["sql/setup/layer.sql"]],
            object = query![include_str!["sql/setup/object.sql"]],
            location = query![include_str!["sql/setup/location.sql"]],
            session_triggers = query![include_str!("sql/setup/session_triggers.sql")],
            layer_triggers = query![include_str!("sql/setup/layer_triggers.sql")]
        };

        psql.batch_execute(setup_query.as_str())
            .map_err(|err| make_error![Error::Storage::SetupSchema(err)])
    }

    // pub fn session(&mut self) -> Session {
    //     Session::new_api(self)
    // }

    // pub fn layer(&mut self) -> Result<Layer> {
    //     self.transaction().map(|transaction| Layer::new_api(transaction))
    // }
    // pub fn object(&mut self) -> Object {
    //     Object::new_api(self)
    // }
}

pub trait StorageTransaction<'storage> {
    fn session<'t>(&'t mut self) -> session::Session<'t, 'storage>;

    fn layer<'t>(&'t mut self) -> layer::Layer<'t, 'storage>;
}

impl<'storage> StorageTransaction<'storage> for Transaction<'storage> {
    fn session<'t>(&'t mut self) -> session::Session<'t, 'storage> {
        session::Session::new_api(self)
    }

    fn layer<'t>(&'t mut self) -> layer::Layer<'t, 'storage> {
        layer::Layer::new_api(self)
    }
}