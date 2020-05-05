use {
    std::fmt,
    log::error,
    r2d2_postgres::PostgresConnectionManager,
    r2d2_sqlite::SqliteConnectionManager,
    crate::{
        app, 
        make_error, 
        Result,
        r#type::{
            ObjectId,
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

#[derive(Clone)]
pub struct StorageManager {
    pub(in crate::storage) pool: r2d2::Pool<PostgresConnectionManager<postgres::NoTls>>
}

impl StorageManager {
    pub fn setup(connection_string: &str) -> Result<Self> {
        let mgr = PostgresConnectionManager::new(
            connection_string.parse()?, 
            postgres::NoTls
        );

        let pool = r2d2::Pool::new(mgr)?;
        {
            let mut client = pool.get()?;
            Self::setup_schema(&mut client)?;
        }

        let storage_mgr = Self {
            pool
        };

        Ok(storage_mgr)
    }

    fn setup_schema(psql: &mut postgres::Client) -> Result<()> {
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

#[derive(Clone)]
pub struct OccupiedSpacesStorage {
    pool: r2d2::Pool<SqliteConnectionManager>
}

impl OccupiedSpacesStorage {
    pub fn new() -> Result<Self> {
        let mgr = SqliteConnectionManager::memory();
        let pool = r2d2::Pool::new(mgr)?;

        pool.get()?.execute(
            include_str!["sql/setup/oss/occupied_space.sql"],
            rusqlite::NO_PARAMS
        ).map_err(|err| make_error![Error::Storage::OccupiedSpacesStorageInit(err)])?;

        let oss = Self {
            pool
        };

        Ok(oss)
    } 

    pub fn add_occupied_space(&self, occupied_space: OccupiedSpace) -> Result<()> {
        let connection = self.pool.get()?;
        let mut stmt = connection.prepare_cached(include_str![
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
        let connection = self.pool.get()?;
        let mut stmt = connection.prepare_cached(include_str![
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