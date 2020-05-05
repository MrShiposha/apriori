use {
    std::fmt,
    log::error,
    crate::{
        app, 
        make_error, 
        Result,
        r#type::{
            ObjectId,
            OccupiedSpaceId,
            Coord,
            Distance,
            RelativeTime,
            Vector,
        }
    }
};

pub mod session;
pub mod object;
pub mod attractor;

pub use object::Object;
pub use attractor::Attractor;
pub use session::Session;

const LOG_TARGET: &'static str = "storage";

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
            schema = query![include_str!("sql/setup/master/schema.sql")],
            session = query! {
                include_str!("sql/setup/master/session.sql"),
                session_max_hang_time = app::SESSION_MAX_HANG_TIME.num_seconds()
            },
            object = query![include_str!["sql/setup/master/object.sql"]],
            location = query![include_str!["sql/setup/master/location.sql"]],
            attractor = query![include_str!["sql/setup/master/attractor.sql"]],
            session_triggers = query![include_str!("sql/setup/master/session_triggers.sql")]
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

pub struct OccupiedSpacesStorage {
    connection: rusqlite::Connection
}

impl OccupiedSpacesStorage {
    pub fn new() -> Result<Self> {
        let connection = rusqlite::Connection::open_in_memory()
            .map_err(|err| make_error![Error::Storage::OccupiedSpacesStorageInit(err)])?;

        connection.execute(
            include_str!["sql/setup/oss/occupied_space.sql"],
            rusqlite::NO_PARAMS
        ).map_err(|err| make_error![Error::Storage::OccupiedSpacesStorageInit(err)])?;

        let oss = Self {
            connection
        };

        Ok(oss)
    } 

    pub fn add_occupied_space(&self, occupied_space: OccupiedSpace) -> Result<()> {
        let mut stmt = self.connection.prepare_cached(include_str![
            "sql/oss/add_occupied_space.sql"
        ]).map_err(|err| make_error![Error::Storage::AddOccupiedSpace(err)])?;

        let bv = occupied_space.begin_velocity;
        let bvx = bv[0] as f64;
        let bvy = bv[1] as f64;
        let bvz = bv[2] as f64;

        let ev = occupied_space.end_velocity;
        let evx = ev[0] as f64;
        let evy = ev[1] as f64;
        let evz = ev[2] as f64;

        stmt.execute(rusqlite::params![
            occupied_space.x_min as f64, occupied_space.x_max as f64,
            occupied_space.y_min as f64, occupied_space.y_max as f64,
            occupied_space.z_min as f64, occupied_space.z_max as f64,
            occupied_space.t_min as f64, occupied_space.t_max as f64,
            occupied_space.object_id,
            bvx, bvy, bvz,
            evx, evy, evz

        ]).map_err(|err| make_error![Error::Storage::AddOccupiedSpace(err)])?;

        Ok(())
    }

    pub fn check_possible_collisions(
        &self, 
        occupied_space: &OccupiedSpace
    ) -> Result<Vec<OccupiedSpace>> {
        let mut stmt = self.connection.prepare_cached(include_str![
            "sql/oss/check_possible_collisions.sql"
        ]).map_err(|err| make_error![Error::Storage::CheckPossibleCollisions(err)])?;


        let rows = stmt.query(rusqlite::params![
            occupied_space.x_min as f64, occupied_space.x_max as f64,
            occupied_space.y_min as f64, occupied_space.y_max as f64,
            occupied_space.z_min as f64, occupied_space.z_max as f64,
            occupied_space.t_min as f64, occupied_space.t_max as f64,
            occupied_space.object_id,
        ]).map_err(|err| make_error![Error::Storage::CheckPossibleCollisions(err)])?;

        let possible_collisions = rows
            .and_then(|row| OccupiedSpace::with_row(row))
            .into_iter()
            .filter_map(|res| match res {
                Ok(os) => Some(os),
                Err(err) => {
                    error! {
                        target: LOG_TARGET,
                        "unable to read occupied space from row: {}", err
                    }

                    None
                }
            })
            .collect();

        Ok(possible_collisions)
    }
}

pub struct OccupiedSpace {
    pub object_id: ObjectId,
    pub x_min: Coord, pub x_max: Coord,
    pub y_min: Coord, pub y_max: Coord,
    pub z_min: Coord, pub z_max: Coord,
    pub t_min: Coord, pub t_max: Coord,
    pub begin_velocity: Vector,
    pub end_velocity: Vector,
}

impl OccupiedSpace {
    pub fn with_track_part(
        object_id: ObjectId,
        cube_size: Distance, 
        begin_location: &Vector, 
        begin_time: RelativeTime,
        begin_velocity: Vector,
        end_location: &Vector,
        end_time: RelativeTime,
        end_velocity: Vector,
    ) -> Self {
        macro_rules! min_max {
            ($a:expr, $b:expr $(, +/- $cube_size:expr)?) => {
                (
                    $a.min($b) $(- $cube_size)?,
                    $a.max($b) $(+ $cube_size)?,
                )
            };
        }

        let x_0 = begin_location[0];
        let y_0 = begin_location[1];
        let z_0 = begin_location[2];

        let x_1 = end_location[0];
        let y_1 = end_location[1];
        let z_1 = end_location[2];

        let (x_min, x_max) = min_max![x_0, x_1, +/- cube_size];
        let (y_min, y_max) = min_max![y_0, y_1, +/- cube_size];
        let (z_min, z_max) = min_max![z_0, z_1, +/- cube_size];
        let (t_min, t_max) = min_max![begin_time, end_time];

        Self {
            object_id,
            x_min, x_max,
            y_min, y_max,
            z_min, z_max,
            t_min, t_max,
            begin_velocity,
            end_velocity,
        }
    }

    // pub fn with_location(
    //     object_id: ObjectId, 
    //     object_radius: Distance, 
    //     location: Vector,
    //     time: RelativeTime,
    // ) -> Self {
    //     let x_min = location[0] - object_radius;
    //     let x_max = location[0] + object_radius;

    //     let y_min = location[1] - object_radius;
    //     let y_max = location[1] + object_radius;

    //     let z_min = location[2] - object_radius;
    //     let z_max = location[2] + object_radius;
        
    //     let t_min = time;
    //     let t_max = t_min;

    //     Self {
    //         object_id,
    //         x_min, x_max,
    //         y_min, y_max,
    //         z_min, z_max,
    //         t_min, t_max,
    //     }
    // }

    pub fn with_row(row: &rusqlite::Row<'_>) -> Result<Self> {
        macro_rules! get {
            ($idx:literal raw_type) => {
                row.get($idx)
                    .map_err(|err| make_error![Error::Storage::ReadOccupiedSpace(err)])?
            };

            ($idx:literal) => {{
                let col: f64 = row.get($idx)
                    .map_err(|err| make_error![Error::Storage::ReadOccupiedSpace(err)])?;

                col as f32
            }};


        }

        let os = Self {
            object_id: get![0 raw_type],
            x_min: get![1], 
            x_max: get![2],
            y_min: get![3], 
            y_max: get![4],
            z_min: get![5], 
            z_max: get![6],
            t_min: get![7], 
            t_max: get![8],
            begin_velocity: Vector::new(
                get![9], 
                get![10],
                get![11],
            ),
            end_velocity: Vector::new(
                get![12], 
                get![13],
                get![14],
            ),
        };

        Ok(os)
    }
}

impl fmt::Display for OccupiedSpace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f, 
            "x ∈ [{}, {}), y ∈ [{}, {}), z ∈ [{}, {}), t ∈ [{}, {})",
            self.x_min, self.x_max,
            self.y_min, self.y_max,
            self.z_min, self.z_max,
            self.t_min, self.t_max
        )
    }
}