use {
    crate::{
        engine::context::Context,
        r#type::{AsTimeMBR, Coord, ObjectId, Vector},
    },
    kiss3d::scene::SceneNode,
    log::{trace, warn},
    nalgebra::Translation3,
    std::collections::{HashMap, HashSet},
};

const LOG_TARGET: &'static str = "scene";

pub struct Scene {
    root: SceneNode,
    objects_map: HashMap<ObjectId, SceneNode>,
}

impl Scene {
    pub fn new(root: SceneNode) -> Self {
        Self {
            root,
            objects_map: HashMap::new(),
        }
    }

    pub fn clear(&mut self) {
        self.objects_map
            .iter_mut()
            .for_each(|(_, node)| node.unlink());
        self.objects_map.clear();
    }

    pub fn update(&mut self, new_context: &mut Context) {
        while let Some(id) = new_context.take_new_object_id() {
            let actor = new_context.actor(&id);

            let radius = actor.object().radius();
            let color = actor.object().color();

            let mut sphere = self.root.add_sphere(radius);
            sphere.set_color(color[0], color[1], color[2]);

            match actor.last_gen_coord() {
                Some(last_location) => {
                    let translation = make_translation(last_location.location().clone());

                    sphere.set_local_translation(translation);
                }
                None => sphere.set_visible(false),
            }

            self.objects_map.insert(id, sphere);
        }
    }

    pub fn set_time(&mut self, context: &Context, vtime: chrono::Duration) {
        let mut visited_objects = HashSet::new();

        let mbr = vtime.as_mbr();
        context.tracks_tree().search_access(&mbr, |obj_space, id| {
            let track_part_mbr = obj_space.get_data_mbr(id);
            let track_part_info = obj_space.get_data_payload(id);

            let object_id = track_part_info.object_id;

            let location = context.location(
                track_part_mbr,
                track_part_info,
                mbr.bounds(0).min, // or `.max` - it doesn't matter.
            );

            self.set_obj_translation(&object_id, location);

            visited_objects.insert(object_id);
        });

        let diff = context.actors().keys().filter_map(|id| {
            if visited_objects.contains(id) {
                None
            } else {
                Some(id)
            }
        });

        for unvisited_id in diff {
            let actor = context.actor(unvisited_id);
            let last_coord = actor.last_gen_coord().unwrap();

            if last_coord.time() > vtime {
                trace! {
                    target: LOG_TARGET,
                    "object \"{}\" is not yet appeared",
                    actor.object().name()
                }

                self.hide_object(unvisited_id);
            } else {
                warn! {
                    target: LOG_TARGET,
                    "object \"{}\" is not yet computed",
                    actor.object().name()
                }

                self.set_obj_translation(unvisited_id, last_coord.location().clone());
            }
        }
    }

    fn set_obj_translation(&mut self, id: &ObjectId, location: Vector) {
        let node = self.objects_map.get_mut(id).unwrap();

        node.set_local_translation(make_translation(location));

        node.set_visible(true);
    }

    fn hide_object(&mut self, id: &ObjectId) {
        let node = self.objects_map.get_mut(id).unwrap();

        node.set_visible(false);
    }
}

fn make_translation(location: Vector) -> Translation3<Coord> {
    Translation3::new(location[0], location[1], location[2])
}
