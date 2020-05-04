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
        scene::physics::Engine,
        r#type::{
            ObjectId,
            ObjectName,
            AttractorName,
            Vector,
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
    objects: HashMap<ObjectName, (ObjectId, Arc<RwLock<Object4d>>, SceneNode)>,
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

                let (id, object) = engine.add_object(
                    object, 
                    msg.step, 
                    msg.location,
                )?;

                entry.insert((id, object, node));
                Ok(())
            }
        }
    }

    pub fn add_attractor(
        &mut self, 
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
                let id = engine.add_attractor(attractor, attractor_name)?;

                entry.insert(id);
                Ok(())
            }
        }
    }

    pub fn query_objects_by_time<F: FnMut(&Object4d, Vector)>(
        &mut self, 
        engine: &mut Engine,
        vtime: &chrono::Duration, 
        mut object_handler: F
    ) {
        match engine.update_objects(vtime) {
            Ok(()) => for (name, (_id, object, node)) in self.objects.iter_mut() {
                let sync_object = object.read().unwrap();
    
                match sync_object.track().interpolate(vtime) {
                    Ok(obj_location) => {
                        node.set_local_translation(obj_location.into());
                        object_handler(&*sync_object, obj_location);
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

    pub fn rename_object(
        &mut self, 
        engine: &mut Engine, 
        old_name: ObjectName, 
        new_name: ObjectName
    ) -> Result<()> {
        if old_name == new_name {
            return Ok(());
        }

        let (id, object, node) = self.objects.remove(&old_name)
            .ok_or(make_error![Error::Scene::ObjectNotFound(old_name.clone())])?;
        
        match engine.rename_object_in_master_storage(id, new_name.as_str()) {
            Ok(()) => {
                object.write().unwrap().rename(new_name.clone());
                self.objects.insert(new_name, (id, object, node));

                Ok(())
            }
            Err(err) => {
                self.objects.insert(old_name, (id, object, node));

                Err(err)
            }
        }
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