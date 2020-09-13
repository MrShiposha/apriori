use {
    crate::{
        engine::context::Context,
        r#type::ObjectId,
    },
    std::collections::HashMap,
    kiss3d::scene::SceneNode,
    nalgebra::Translation3,
};

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

    pub fn update(&mut self, new_context: &mut Context) {
        while let Some(id) = new_context.take_new_object_id() {
            let actor = new_context.actor(id);

            let radius = actor.object().radius();
            let last_location = actor.last_gen_coord()
                .unwrap()
                .location()
                .clone();
            let translation = Translation3::new(
                last_location[0],
                last_location[1],
                last_location[2],
            );

            let mut sphere = self.root.add_sphere(radius);
            sphere.set_local_translation(translation);

            self.objects_map.insert(id, sphere);
        }
    }
}