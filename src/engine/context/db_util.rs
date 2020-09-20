use {
    crate::{
        engine::context::{TimeRange, TrackPartInfo},
        graphics,
        object::{GenCoord, Object},
        r#type::{
            AsRelativeTime, Coord, Distance, IntoRustDuration, IntoStorageDuration, LocationId, ObjectId, RawTime,
            Vector, LayerId,
        },
    },
    lr_tree::*,
    serde::{
        Serialize, Serializer,
        Deserialize, Deserializer,
        ser::SerializeTuple,
        de::{SeqAccess, Visitor},
    },
};

const OBJECT_FIELDS_LEN: usize = 6;
const LOCATION_INFO_FIELDS_LEN: usize = 12;

pub struct LocationInfo {
    pub layer_id: LayerId,
    pub location_id: LocationId,
    pub object_id: ObjectId,
    pub t: chrono::Duration,
    pub x: Coord,
    pub y: Coord,
    pub z: Coord,
    pub vx: Coord,
    pub vy: Coord,
    pub vz: Coord,

    pub vcx: Option<Coord>, // vx after collision
    pub vcy: Option<Coord>, // vy after collision
    pub vcz: Option<Coord>, // vz after collision
}

impl LocationInfo {
    pub fn is_after_collision(&self) -> bool {
        self.vcx.is_some()
    }
}

impl Serialize for LocationInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer
    {
        let mut tuple_seq = serializer.serialize_tuple(LOCATION_INFO_FIELDS_LEN)?;

        tuple_seq.serialize_element(&self.object_id)?;
        tuple_seq.serialize_element(&self.layer_id)?;
        tuple_seq.serialize_element(&self.t.into_storage_duration())?;
        tuple_seq.serialize_element(&self.x)?;
        tuple_seq.serialize_element(&self.y)?;
        tuple_seq.serialize_element(&self.z)?;
        tuple_seq.serialize_element(&self.vx)?;
        tuple_seq.serialize_element(&self.vy)?;
        tuple_seq.serialize_element(&self.vz)?;
        tuple_seq.serialize_element(&self.vcx)?;
        tuple_seq.serialize_element(&self.vcy)?;
        tuple_seq.serialize_element(&self.vcz)?;

        tuple_seq.end()
    }
}

impl<'de> Deserialize<'de> for LocationInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ObjectVisitor;

        impl<'de> Visitor<'de> for ObjectVisitor {
            type Value = LocationInfo;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "a location info")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let location_id = seq.next_element()?.expect("expected location ID");
                let object_id = seq.next_element()?.expect("expected object FK ID");

                let t: RawTime = seq.next_element()?.expect("expected location t");
                let t = t.into_rust_duration();

                let x = seq.next_element()?.expect("expected location x");
                let y = seq.next_element()?.expect("expected location y");
                let z = seq.next_element()?.expect("expected location z");
                let vx = seq.next_element()?.expect("expected location vx");
                let vy = seq.next_element()?.expect("expected location vy");
                let vz = seq.next_element()?.expect("expected location vz");
                let vcx = seq.next_element().unwrap_or(None);
                let vcy = seq.next_element().unwrap_or(None);
                let vcz = seq.next_element().unwrap_or(None);

                let location_info = LocationInfo {
                    layer_id: LayerId::default(),
                    location_id,
                    object_id,
                    t,
                    x,
                    y,
                    z,
                    vx,
                    vy,
                    vz,
                    vcx,
                    vcy,
                    vcz,
                };

                Ok(location_info)
            }
        }

        deserializer.deserialize_tuple(LOCATION_INFO_FIELDS_LEN, ObjectVisitor)
    }
}

pub struct ObjectInfo(pub ObjectId, pub Object);

impl<'de> Deserialize<'de> for ObjectInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ObjectInfoVisitor;

        impl<'de> Visitor<'de> for ObjectInfoVisitor {
            type Value = ObjectInfo;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "a session object")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let object_id = seq.next_element()?.expect("expected object ID");

                let layer_id = seq.next_element()?.expect("expected layer ID");
                let name = seq.next_element()?.expect("expected name");
                let radius = seq.next_element()?.expect("expected radius");

                let color = seq.next_element()?.expect("expected color");
                let color = graphics::unpack_color(&color);

                let mass = seq.next_element()?.expect("expected mass");
                let compute_step: RawTime = seq.next_element()?.expect("expected compute step");
                let compute_step = compute_step.into_rust_duration();

                let object = Object::new(layer_id, name, radius, color, mass, compute_step);

                Ok(ObjectInfo(object_id, object))
            }
        }

        deserializer.deserialize_tuple(OBJECT_FIELDS_LEN, ObjectInfoVisitor)
    }
}

pub fn make_last_gen_coord(location_info: &LocationInfo) -> GenCoord {
    let time = location_info.t;
    let location = Vector::new(location_info.x, location_info.y, location_info.z);
    let velocity;

    if location_info.is_after_collision() {
        velocity = Vector::new(location_info.vcx.unwrap(), location_info.vcy.unwrap(), location_info.vcz.unwrap(),);
    } else {
        velocity = Vector::new(location_info.vx, location_info.vy, location_info.vz);
    }

    GenCoord::new(time, location, velocity)
}

pub fn make_track_part_info(
    object_id: ObjectId,
    last_coord: GenCoord,
    location_info: LocationInfo,
) -> TrackPartInfo {
    let final_velocity = if !location_info.is_after_collision() {
        None
    } else {
        Some(
            Vector::new(
                location_info.vcx.unwrap(),
                location_info.vcy.unwrap(),
                location_info.vcz.unwrap(),
            )
        )
    };

    TrackPartInfo {
        object_id,
        start_location: last_coord.location().clone(),
        end_location: Vector::new(location_info.x, location_info.y, location_info.z),
        start_velocity: last_coord.velocity().clone(),
        end_velocity: Vector::new(location_info.vx, location_info.vy, location_info.vz),
        final_velocity,
    }
}

pub fn make_track_part_mbr(
    time_range: &TimeRange,
    object_radius: Distance,
    local_track_part_info: &TrackPartInfo,
) -> MBR<Coord> {
    macro_rules! min_max {
        ($a:expr, $b:expr) => {
            if $a < $b {
                min_max![adjust $a, $b]
            } else {
                min_max![adjust $b, $a]
            }
        };

        (adjust $min:expr, $max:expr) => {
            ($min - object_radius, $max + object_radius)
        };
    }

    let time_start = time_range.start().as_relative_time();
    let time_end = time_range.end().as_relative_time();

    debug_assert_ne!(time_start, time_end);

    let start_location = local_track_part_info.start_location;
    let end_location = local_track_part_info.end_location;

    let (x_min, x_max) = min_max![start_location[0], end_location[0]];

    let (y_min, y_max) = min_max![start_location[1], end_location[1]];

    let (z_min, z_max) = min_max![start_location[2], end_location[2]];

    let mbr = mbr! {
        t = [time_start; time_end],
        x = [x_min; x_max],
        y = [y_min; y_max],
        z = [z_min; z_max]
    };

    mbr
}
