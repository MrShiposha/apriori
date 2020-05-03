use {
    std::{
        collections::{
            HashMap,
            hash_map::Entry,
        },
        sync::{
            Arc,
            Mutex,
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
        info,
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
            AttractorName,
            AttractorId,
            ObjectName,
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

const LOG_TARGET: &'static str = "Scene";

pub struct SceneManager {
    objects: HashMap<ObjectName, (Arc<Mutex<Object4d>>, SceneNode)>,
    attractors: HashMap<AttractorName, Arc<Attractor>>,
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
        session_id: SessionId,
        mut storage: storage::Object<'_>,
        msg: &AddObject, 
        default_name: &ObjectName,
    ) -> Result<()> {
        let obj_name = msg.name.as_ref().unwrap_or(default_name);

        match self.objects.entry(obj_name.clone()) {
            Entry::Occupied(_) => Err(make_error![Error::Scene::ObjectAlreadyExists(obj_name.clone())]),
            Entry::Vacant(entry) => {
                let mut node = self.scene.add_sphere(msg.radius);
                
                let color = msg.color.unwrap_or(random_color());
                node.set_color(color[0], color[1], color[2]);
                node.set_local_translation(msg.location.into());

                let mut track = Track::new(msg.track_size, msg.step);
                let object_mass = msg.mass;

                let attractors = Self::attractors_refs_copy(&self.attractors);
                let mut atom = TrackAtom::with_location(msg.location.clone());
                Engine::make_new_atom(
                    object_mass,
                    &mut atom,
                    track.relative_compute_step(), 
                    attractors
                )?;

                track.push_back(atom.into());

                let object_id = storage.add(session_id, msg, default_name)?;

                let object = Object4d::new(
                    object_id, 
                    track,
                    msg.mass,
                    msg.radius,
                    color
                );

                entry.insert((Arc::new(Mutex::new(object)), node));
                Ok(())
            }
        }
    }

    pub fn add_attractor(&mut self, attractor_id: AttractorId, msg: &AddAttractor, default_name: &AttractorName) -> Result<()> {
        let attr_name = msg.name.as_ref().unwrap_or(default_name);

        match self.attractors.entry(attr_name.clone()) {
            Entry::Occupied(_) => Err(make_error![Error::Scene::AttractorAlreadyExists(attr_name.clone())]),
            Entry::Vacant(entry) => {
                let mut node = self.scene.add_cube(0.5, 0.5, 0.5);
                node.set_color(1.0, 0.0, 0.0);
                node.set_local_translation(msg.location.into());

                let attractor = Attractor::new(attractor_id, msg.location, msg.mass, msg.gravity_coeff);

                entry.insert(Arc::new(attractor));
                Ok(())
            }
        }
    }

    pub fn query_objects_by_time<F: FnMut(&str, &Object4d, Vector)>(
        &mut self, 
        vtime: &chrono::Duration, 
        engine: &mut Engine,
        mut object_handler: F
    ) {
        let objects = self.objects.iter_mut().map(|(name, (obj, node))| (name, obj, node));
        Self::update_objects_locations(
            objects,
            Self::attractors_refs_copy(&self.attractors),
            engine,
            vtime, 
            &mut object_handler
        );

        // if !uncomputed_objects.is_empty() {
        //     engine.compute(
        //         uncomputed_objects.iter()
        //             .map(|(_, object, _)| Arc::clone(*object))
        //             .collect(),

        //         Self::attractors_refs_copy(&self.attractors)
        //     );

        //     let computed_objects = uncomputed_objects.into_iter();
        //     Self::update_objects_locations(
        //         computed_objects,
        //         engine, 
        //         vtime, 
        //         &mut object_handler
        //     );
        // }
    }

    fn update_objects_locations<'a, I, F>(
        objects: I,
        attractors: Vec<Arc<Attractor>>,
        engine: &mut Engine,
        vtime: &chrono::Duration, 
        object_handler: &mut F
    )
    where 
        I: Iterator<Item=(&'a ObjectName, &'a mut Arc<Mutex<Object4d>>, &'a mut SceneNode)>,
        F: FnMut(&str, &Object4d, Vector)
    {
        let mut objects = objects.collect::<Vec<_>>();

        engine.update_objects(
            vtime, 
            objects.iter().map(|(_, object, _)| Arc::clone(object)).collect(), 
            attractors,
        );

        for (name, object, node) in objects.iter_mut() {
            let sync_object = object.lock().unwrap();

            let obj_location = sync_object.track().interpolate(vtime).unwrap();
            node.set_local_translation(obj_location.into());
            object_handler(name.as_str(), &*sync_object, obj_location);
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