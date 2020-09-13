use {
    crate::{
        Result,
        engine::context::Context,
        error::{Error, Interpolation},
        r#type::{ObjectId, Vector, Coord},
    },
    std::collections::HashMap,
    kiss3d::scene::SceneNode,
    nalgebra::Translation3,
    log::{trace, warn}
};

const LOG_TARGET: &'static str = "scene";

pub struct Scene {
    root: SceneNode,
    objects_map: HashMap<ObjectId, SceneNode>
}

impl Scene {
    pub fn new(root: SceneNode) -> Self {
        Self {
            root,
            objects_map: HashMap::new(),
        }
    }

    pub fn clear(&mut self) {
        self.objects_map.iter_mut().for_each(|(_, node)| node.unlink());
        self.objects_map.clear();
    }

    pub fn update(&mut self, new_context: &mut Context) {
        while let Some(id) = new_context.take_new_object_id() {
            let actor = new_context.actor(id);

            let radius = actor.object().radius();
            let color = actor.object().color();

            let mut sphere = self.root.add_sphere(radius);
            sphere.set_color(color[0], color[1], color[2]);

            match actor.last_gen_coord() {
                Some(last_location) => {
                    let translation = make_translation(last_location.location().clone());

                    sphere.set_local_translation(translation);
                },
                None => sphere.set_visible(false)
            }

            self.objects_map.insert(id, sphere);
        }
    }

    pub fn set_time(&mut self, context: &Context, vtime: chrono::Duration) -> Result<()> {
        for (id, actor) in context.actors() {
            match actor.location(vtime) {
                Ok(location) => {
                    self.set_obj_translation(id, location);
                }
                Err(Error::Interpolation(Interpolation::NoTrackParts)) => {
                    warn! {
                        target: LOG_TARGET,
                        "object \"{}\" has no track parts",
                        actor.object().name()
                    }

                    self.hide_object(id);
                }
                Err(Error::Interpolation(Interpolation::ObjectIsNotComputed(last_location))) => {
                    warn! {
                        target: LOG_TARGET,
                        "object \"{}\" is not yet computed",
                        actor.object().name()
                    }

                    self.set_obj_translation(id, last_location);
                }
                Err(Error::Interpolation(Interpolation::FutureObject)) => {
                    trace! {
                        target: LOG_TARGET,
                        "object \"{}\" is not yet appeared",
                        actor.object().name()
                    }

                    self.hide_object(id);
                },
                Err(err) => return Err(err)
            }
        }

        Ok(())
    }

    fn set_obj_translation(&mut self, id: &ObjectId, location: Vector) {
        let node = self.objects_map.get_mut(id).unwrap();

        node.set_local_translation(
            make_translation(location)
        );

        node.set_visible(true);
    }

    fn hide_object(&mut self, id: &ObjectId) {
        let node = self.objects_map.get_mut(id).unwrap();

        node.set_visible(false);
    }
}

fn make_translation(location: Vector) -> Translation3<Coord> {
    Translation3::new(
        location[0],
        location[1],
        location[2],
    )
}