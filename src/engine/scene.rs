use {
    crate::{
        engine::context::{Context, TrackPartInfo},
        r#type::{AsTimeMBR, Coord, ObjectId, ObjectName, Vector},
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
    rtree: Option<SceneNode>,
}

impl Scene {
    pub fn new(root: SceneNode) -> Self {
        Self {
            root,
            objects_map: HashMap::new(),
            rtree: None,
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

        match self.rtree {
            Some(ref mut rtree) => {
                if rtree.is_visible() {
                    self.create_rtree(new_context);
                } else {
                    rtree.unlink();
                }
            }
            _ => {}
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
            match actor.last_gen_coord() {
                None => self.handle_future_object(unvisited_id, actor.object().name()),
                Some(last_coord) if last_coord.time() > vtime => {
                    self.handle_future_object(unvisited_id, actor.object().name());
                },
                Some(last_coord) => {
                    warn! {
                        target: LOG_TARGET,
                        "object \"{}\" is not yet computed",
                        actor.object().name()
                    }

                    self.set_obj_translation(unvisited_id, last_coord.location().clone());
                }
            }
        }
    }

    pub fn create_rtree(&mut self, context: &Context) {
        if let Some(ref mut rtree) = self.rtree {
            rtree.unlink();
        }

        self.rtree = Some(self.root.add_group());

        let mut visitor = RTreeVisitor {
            scene: self,
            context
        };

        context.tracks_tree().visit(&mut visitor);
    }

    pub fn has_rtree(&mut self) -> bool {
        self.rtree.is_some()
    }

    pub fn show_rtree(&mut self) {
        self.rtree
            .as_mut()
            .expect("rtree node must be already created")
            .set_visible(true)
    }

    pub fn hide_rtree(&mut self) {
        if let Some(ref mut rtree) = self.rtree {
            rtree.set_visible(false);
        }
    }

    fn handle_future_object(&mut self, object_id: &ObjectId, obj_name: &ObjectName) {
        trace! {
            target: LOG_TARGET,
            "object \"{}\" is not yet appeared",
            obj_name
        }

        self.hide_object(object_id);
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

struct RTreeVisitor<'scene, 'ctx> {
    scene: &'scene mut Scene,
    context: &'ctx Context,
}

impl<'node, 'ctx> lr_tree::Visitor<Coord, TrackPartInfo> for RTreeVisitor<'node, 'ctx> {
    fn enter_node(&mut self, _: lr_tree::RecordId, node: &lr_tree::InternalNode<Coord>) {
        make_cube_from_mbr(self.scene.rtree.as_mut().unwrap(), node.mbr());
    }

    fn leave_node(&mut self, _: lr_tree::RecordId, _: &lr_tree::InternalNode<Coord>) {
        // do nothing
    }

    fn visit_data(&mut self, _: lr_tree::RecordId, node: &lr_tree::DataNode<Coord, TrackPartInfo>) {
        let mut cube = make_cube_from_mbr(
            self.scene.rtree.as_mut().unwrap(),
            node.mbr()
        );

        let color = self.context.actor(&node.payload().object_id).object().color();
        cube.set_color(color[0], color[1], color[2]);
    }
}

fn make_cube_from_mbr(node: &mut SceneNode, mbr: &lr_tree::MBR<Coord>) -> SceneNode {
    // axis_index == 0 -- it is a time.
    let x_bounds = mbr.bounds(1);
    let y_bounds = mbr.bounds(2);
    let z_bounds = mbr.bounds(3);

    let x_len = x_bounds.length();
    let y_len = y_bounds.length();
    let z_len = z_bounds.length();

    let x = x_bounds.min + x_len / 2.0;
    let y = y_bounds.min + y_len / 2.0;
    let z = z_bounds.min + z_len / 2.0;

    let mut cube = node.add_cube(
        x_len,
        y_len,
        z_len
    );

    cube.set_local_translation(
        Translation3::new(x, y, z)
    );

    cube.set_points_size(10.0);
    cube.set_lines_width(1.0);
    cube.set_surface_rendering_activation(false);

    cube
}