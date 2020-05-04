use {
    std::{
        collections::{
            HashMap,
            hash_map::Entry,
        },
        sync::{
            Arc,
            RwLock,
        },
        ops::{
            RangeFrom,
            RangeTo
        }
    },
    kiss3d::{
        scene::SceneNode,
    },
    log::{
        error,
    },
    crate::{
        Result,
        make_error,
        message::{
            AddAttractor,
            AddObject,
        },
        graphics::random_color,
        scene::{
            track::{
                Track,
                TrackAtom,
            },
            physics::{
                Engine,
                TimeDirection,
            },
        },
        storage,
        r#type::{
            SessionId,
            ObjectId,
            ObjectName,
            AttractorId,
            AttractorName,
            Vector,
            TimeFormat
        },
    }
};

mod object;
mod track;
mod attractor;
mod ringbuffer;
pub mod physics;

pub use object::Object4d;
pub use attractor::Attractor;

const LOG_TARGET: &'static str = "scene";

pub struct SceneManager {
    objects: HashMap<ObjectName, (Arc<RwLock<Object4d>>, SceneNode)>,
    attractors: HashMap<AttractorName, Arc<RwLock<Attractor>>>,
    scene: SceneNode,
}

impl SceneManager {
    pub fn new(scene_root: SceneNode) -> Self {
        Self {
            objects: HashMap::new(),
            attractors: HashMap::new(),
            scene: scene_root,
        }
    }

    pub fn add_object(
        &mut self,
        storage: storage::Object<'_>,
        session_id: SessionId,
        engine: &mut Engine,
        msg: AddObject,
        default_name: ObjectName,
    ) -> Result<()> {
        let obj_name = msg.name.unwrap_or(default_name);

        match self.objects.entry(obj_name.clone()) {
            Entry::Occupied(_) => Err(make_error![Error::Scene::ObjectAlreadyExists(obj_name.clone())]),
            Entry::Vacant(entry) => {
                let mut node = self.scene.add_sphere(msg.radius);
                
                let color = msg.color.unwrap_or(random_color());
                node.set_color(color[0], color[1], color[2]);
                node.set_local_translation(msg.location.into());

                let object = Object4d::new(
                    msg.track_size,
                    msg.step,
                    obj_name,
                    msg.mass,
                    msg.radius,
                    color
                );

                let object = engine.add_object(
                    storage, 
                    session_id, 
                    object, 
                    msg.step, 
                    msg.location,
                )?;

                entry.insert((object, node));
                Ok(())
            }
        }
    }

    pub fn add_attractor(
        &mut self, 
        storage: storage::Attractor<'_>,
        session_id: SessionId,
        engine: &mut Engine,
        msg: AddAttractor, 
        default_name: AttractorName
    ) -> Result<()> {
        let attractor_name = msg.name.unwrap_or(default_name);

        match self.attractors.entry(attractor_name.clone()) {
            Entry::Occupied(_) => Err(make_error![Error::Scene::AttractorAlreadyExists(attractor_name.clone())]),
            Entry::Vacant(entry) => {
                let mut node = self.scene.add_cube(0.5, 0.5, 0.5);
                node.set_color(1.0, 0.0, 0.0);
                node.set_local_translation(msg.location.into());

                let attractor = Attractor::new(msg.location, msg.mass, msg.gravity_coeff);
                let id = engine.add_attractor(storage, session_id, attractor, attractor_name)?;

                entry.insert(id);
                Ok(())
            }
        }
    }

    pub fn query_objects_by_time<F: FnMut(&str, &Object4d, Vector)>(
        &mut self, 
        engine: &mut Engine,
        vtime: &chrono::Duration, 
        mut object_handler: F
    ) {
        match engine.update_objects(vtime) {
            Ok(()) => for (name, (object, node)) in self.objects.iter_mut() {
                let sync_object = object.read().unwrap();
    
                match sync_object.track().interpolate(vtime) {
                    Ok(obj_location) => {
                        node.set_local_translation(obj_location.into());
                        object_handler(name.as_str(), &*sync_object, obj_location);
                    },
                    Err(err) => error! {
                        target: LOG_TARGET,
                        "unable to interpolate object `{}`: {}", name, err
                    }
                }
            },
            Err(err) => error! {
                target: LOG_TARGET,
                "unable to update objects on the scene: {}", err
            }
        }
    }

    fn attractors_refs_copy(attractors: &HashMap<String, Arc<Attractor>>) -> Vec<Arc<Attractor>> {
        attractors.values()
            .into_iter()
            .map(|attr| Arc::clone(attr))
            .collect()
    }
}

pub enum TruncateRange<T> {
    From(T),
    To(T),
}

impl<T> TruncateRange<T> {
    pub fn map<U>(&self, f: impl Fn(&T) -> U) -> TruncateRange<U> {
        match self {
            TruncateRange::From(index) => TruncateRange::From(f(index)),
            TruncateRange::To(index) => TruncateRange::To(f(index))
        }
    }
}

impl<T> From<RangeFrom<T>> for TruncateRange<T> {
    fn from(range: RangeFrom<T>) -> Self {
        Self::From(range.start)
    }
}

impl<T> From<RangeTo<T>> for TruncateRange<T> {
    fn from(range: RangeTo<T>) -> Self {
        Self::To(range.end)
    }
}