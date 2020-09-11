use {
    crate::{
        r#type::{
            SessionId,
            LayerId,
            ObjectId,
            ObjectName,
            Distance,
            Color,
            Mass,
            IntoRustDuration,
            IntoStorageDuration,
            RelativeTime,
            AsRelativeTime,
            AsAbsoluteTime,
            RawTime,
            Vector,
        },
        graphics
    },
    std::{
        hash::{Hash, Hasher},
        borrow::Borrow,
    },
    serde::{Serialize, Serializer, Deserialize, Deserializer, ser::SerializeTuple, de::{Visitor, SeqAccess}}
};

const OBJECT_FIELDS_LEN: usize = 6;
const GEN_COORD_FIELDS_LEN: usize = 8;

#[derive(Debug, Clone)]
pub struct Object {
    layer_id: LayerId,
    name: ObjectName,
    radius: Distance,
    color: Color,
    mass: Mass,
    compute_step: chrono::Duration,
}

impl Object {
    pub fn new(
        name: ObjectName,
        radius: Distance,
        color: Color,
        mass: Mass,
        compute_step: chrono::Duration
    ) -> Self {
        Self {
            layer_id: LayerId::default(),
            name,
            radius,
            color,
            mass,
            compute_step
        }
    }

    pub fn layer_id(&self) -> LayerId {
        self.layer_id
    }

    pub fn name(&self) -> &ObjectName {
        &self.name
    }

    pub fn radius(&self) -> Distance {
        self.radius
    }

    pub fn color(&self) -> &Color {
        &self.color
    }

    pub fn mass(&self) -> Mass {
        self.mass
    }

    pub fn compute_step(&self) -> chrono::Duration {
        self.compute_step
    }
}

impl Default for Object {
    fn default() -> Self {
        Self {
            layer_id: Default::default(),
            name: Default::default(),
            radius: Default::default(),
            color: Color::origin(),
            mass: Default::default(),
            compute_step: chrono::Duration::zero(),
        }
    }
}

pub struct InitialObjectInfo<'o>(pub SessionId, pub LayerId, pub &'o Object);

impl<'o> Serialize for InitialObjectInfo<'o> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer
    {
        let InitialObjectInfo(
            session_id,
            layer_id,
            object
        ) = self;

        let mut tuple_seq = serializer.serialize_tuple(OBJECT_FIELDS_LEN)?;

        tuple_seq.serialize_element(session_id)?;
        tuple_seq.serialize_element(&layer_id)?;
        tuple_seq.serialize_element(&object.name)?;
        tuple_seq.serialize_element(&object.radius)?;
        tuple_seq.serialize_element(&graphics::pack_color(&object.color))?;
        tuple_seq.serialize_element(&object.mass)?;
        tuple_seq.serialize_element(&object.compute_step.into_storage_duration())?;

        tuple_seq.end()
    }
}

pub struct Entry(pub ObjectId, pub Object);

impl<'de> Deserialize<'de> for Entry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>
    {
        struct ObjectVisitor;

        impl<'de> Visitor<'de> for ObjectVisitor {
            type Value = Entry;

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

                let object = Object {
                    layer_id,
                    name,
                    radius,
                    color,
                    mass,
                    compute_step
                };

                Ok(Entry(object_id, object))
            }
        }

        deserializer.deserialize_tuple(OBJECT_FIELDS_LEN, ObjectVisitor)
    }
}

impl Hash for Object {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl PartialEq for Object {
    fn eq(&self, other: &Self) -> bool {
        self.name.eq(other.name())
    }
}

impl Eq for Object {}

impl Borrow<ObjectName> for Object {
    fn borrow(&self) -> &ObjectName {
        self.name()
    }
}

#[derive(Debug, Clone)]
pub struct GenCoord {
    time: RelativeTime,
    location: Vector,
    velocity: Vector,
}

impl GenCoord {
    pub fn new(time: RelativeTime, location: Vector, velocity: Vector) -> Self {
        Self {
            time,
            location,
            velocity
        }
    }

    pub fn time(&self) -> RelativeTime {
        self.time
    }

    pub fn location(&self) -> &Vector {
        &self.location
    }

    pub fn velocity(&self) -> &Vector {
        &self.velocity
    }
}

pub struct ObjectGenCoord(pub ObjectId, pub GenCoord);

impl Serialize for ObjectGenCoord {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer
    {
        let mut tuple_seq = serializer.serialize_tuple(GEN_COORD_FIELDS_LEN)?;

        let ObjectGenCoord(object_id, coord) = self;

        tuple_seq.serialize_element(object_id)?;
        tuple_seq.serialize_element(&coord.time.as_absolute_time().into_storage_duration())?;
        tuple_seq.serialize_element(&coord.location[0])?;
        tuple_seq.serialize_element(&coord.location[1])?;
        tuple_seq.serialize_element(&coord.location[2])?;
        tuple_seq.serialize_element(&coord.velocity[0])?;
        tuple_seq.serialize_element(&coord.velocity[1])?;
        tuple_seq.serialize_element(&coord.velocity[2])?;

        tuple_seq.end()
    }
}

impl<'de> Deserialize<'de> for ObjectGenCoord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>
    {
        struct ObjectGenCoordVisitor;

        impl<'de> Visitor<'de> for ObjectGenCoordVisitor {
            type Value = ObjectGenCoord;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "an object generalized coordinate")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let object_id = seq.next_element()?.expect("expected object ID");

                let time: RawTime = seq.next_element()?.expect("expected time");
                let time = time.into_rust_duration().as_relative_time();

                let lx = seq.next_element()?.expect("expected x coord");
                let ly = seq.next_element()?.expect("expected y coord");
                let lz = seq.next_element()?.expect("expected z coord");

                let vx = seq.next_element()?.expect("expected x velocity coord");
                let vy = seq.next_element()?.expect("expected x velocity coord");
                let vz = seq.next_element()?.expect("expected x velocity coord");

                let coord = GenCoord::new(
                    time,
                    Vector::new(lx, ly, lz),
                    Vector::new(vx, vy, vz)
                );

                Ok(ObjectGenCoord(object_id, coord))
            }
        }

        deserializer.deserialize_tuple(GEN_COORD_FIELDS_LEN, ObjectGenCoordVisitor)
    }
}