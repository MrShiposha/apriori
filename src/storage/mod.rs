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
            evx, evy, evz,
            occupied_space.cube_size as f64,
            occupied_space.location_info.0

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
    pub t_min: RelativeTime, 
    pub t_max: RelativeTime,
    pub begin_velocity: Vector,
    pub end_velocity: Vector,
    pub cube_size: Distance,
    location_info: LocationInfo,
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

        let mut oss = Self {
            object_id,
            x_min, x_max,
            y_min, y_max,
            z_min, z_max,
            t_min, t_max,
            begin_velocity,
            end_velocity,
            cube_size,
            location_info: LocationInfo(0)
        };

        oss.location_info = LocationInfo::save_locations(begin_location, end_location, &oss);

        oss
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
            cube_size: get![15],
            location_info: LocationInfo(get![16 raw_type]),
        };

        Ok(os)
    }

    pub fn restore_locations(&self) -> (Vector, Vector) {
        self.location_info.restore_locations(self)
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

type LocationFlags = i64;

enum LocationCoordMask {
    BeginX = 0b1,
    BeginY = 0b10,
    BeginZ = 0b100,
    EndX = 0b1000,
    EndY = 0b10000,
    EndZ = 0b100000
}

struct LocationInfo(LocationFlags);

impl LocationInfo {
    fn save_locations(begin_location: &Vector, end_location: &Vector, os: &OccupiedSpace) -> Self {
        let mut flags = 0;

        macro_rules! set_loc_flag {
            ($mask:ident($coord:expr), max: $max:expr) => {
                if $coord == $max - os.cube_size {
                    flags |= LocationCoordMask::$mask as LocationFlags;
                }
            };
        }

        set_loc_flag![BeginX(begin_location[0]), max: os.x_max];
        set_loc_flag![BeginY(begin_location[1]), max: os.y_max];
        set_loc_flag![BeginZ(begin_location[2]), max: os.z_max];

        set_loc_flag![EndX(end_location[0]), max: os.x_max];
        set_loc_flag![EndY(end_location[1]), max: os.y_max];
        set_loc_flag![EndZ(end_location[2]), max: os.z_max];

        LocationInfo(flags)
    }

    fn restore_locations(&self, os: &OccupiedSpace) -> (Vector, Vector) {
        macro_rules! set_min_max {
            ($mask:ident($coord:expr), min: $min:expr, max: $max:expr) => {
                if self.0 & LocationCoordMask::$mask as LocationFlags != 0 {
                    $coord = $max - os.cube_size;
                } else {
                    $coord = $min + os.cube_size;
                }
            };
        }

        let bx;
        let by;
        let bz;

        let ex;
        let ey;
        let ez;

        set_min_max![BeginX(bx), min: os.x_min, max: os.x_max];
        set_min_max![BeginY(by), min: os.y_min, max: os.y_max];
        set_min_max![BeginZ(bz), min: os.z_min, max: os.z_max];

        set_min_max![EndX(ex), min: os.x_min, max: os.x_max];
        set_min_max![EndY(ey), min: os.y_min, max: os.y_max];
        set_min_max![EndZ(ez), min: os.z_min, max: os.z_max];

        let begin_location = Vector::new(bx, by, bz);
        let end_location = Vector::new(ex, ey, ez);

        (begin_location, end_location)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        r#type::Vector,
        storage::{
            OccupiedSpace,
            LocationInfo,
        }
    };

    macro_rules! check_all_info_variants {
        (checker: $checker:ident) => {{
            let x_min = 0.0;
            let x_max = 1.0;
            let y_min = -1.0;
            let y_max = 2.0;
            let z_min = -2.0;
            let z_max = 3.0;

            $checker! {
                begin: Vector::new(x_min, y_min, z_min),
                end: Vector::new(x_max, y_max, z_max),
                info: 0b111000
            }
    
            $checker! {
                begin: Vector::new(x_min, y_min, z_max),
                end: Vector::new(x_max, y_max, z_min),
                info: 0b011100
            }
    
            $checker! {
                begin: Vector::new(x_min, y_max, z_max),
                end: Vector::new(x_max, y_min, z_min),
                info: 0b001110
            }
    
            $checker! {
                begin: Vector::new(x_max, y_max, z_max),
                end: Vector::new(x_min, y_min, z_min),
                info: 0b000111
            }
    
            $checker! {
                begin: Vector::new(x_max, y_min, z_max),
                end: Vector::new(x_min, y_max, z_min),
                info: 0b010101
            }
    
            $checker! {
                begin: Vector::new(x_min, y_max, z_min),
                end: Vector::new(x_max, y_min, z_max),
                info: 0b101010
            }
    
            $checker! {
                begin: Vector::new(x_max, y_max, z_min),
                end: Vector::new(x_min, y_min, z_max),
                info: 0b100011
            }
    
            $checker! {
                begin: Vector::new(x_max, y_min, z_min),
                end: Vector::new(x_min, y_max, z_max),
                info: 0b110001
            }
        }};
    }

    macro_rules! make_oss {
        (begin: $begin:expr, end: $end:expr) => {{
            let x_min = $begin[0].min($end[0]);
            let x_max = $begin[0].max($end[0]);

            let y_min = $begin[1].min($end[1]);
            let y_max = $begin[1].max($end[1]);

            let z_min = $begin[2].min($end[2]);
            let z_max = $begin[2].max($end[2]);

            OccupiedSpace {
                object_id: 0,
                x_min,
                x_max,
                y_min,
                y_max,
                z_min,
                z_max,
                t_min: 0.0,
                t_max: 1.0,
                begin_velocity: Vector::zeros(),
                end_velocity: Vector::zeros(),
                cube_size: 0.0,
                location_info: LocationInfo(0),
            }  
        }};
    }

    #[test]
    fn test_save_locations() {
        macro_rules! check_info {
            (begin: $begin:expr, end: $end:expr, info: $info:expr) => {
                let os = make_oss![begin: $begin, end: $end];

                let loc_info = LocationInfo::save_locations(&$begin, &$end, &os);
                assert_eq!(loc_info.0, $info);
            };
        }
        
        check_all_info_variants![checker: check_info];
    }

    #[test]
    fn test_restore_locations() {
        macro_rules! check_info {
            (begin: $begin:expr, end: $end:expr, info: $info:expr) => {
                let os = make_oss![begin: $begin, end: $end];

                let loc_info = LocationInfo($info);

                let (begin, end) = loc_info.restore_locations(&os);
                assert_eq!($begin, begin);
                assert_eq!($end, end);
            };
        }

        check_all_info_variants![checker: check_info];
    }
}