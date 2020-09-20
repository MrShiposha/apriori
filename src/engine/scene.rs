use {
    crate::{
        engine::{
            DebugInfoSettings,
            context::{Context, TrackPartInfo, TracksSpace, TrackPartId},
        },
        graphics,
        r#type::{AsTimeMBR, Coord, ObjectId, ObjectName, Vector, Color, AsRelativeTime, RelativeTime},
    },
    kiss3d::{scene::SceneNode, window::Window, camera::Camera, text::Font},
    log::{trace, warn},
    nalgebra::{Translation3, Point3, Point2, Vector2},
    std::collections::{HashMap, HashSet, hash_map::Entry},
    lr_tree::mbr,
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
                    self.rtree = None;
                }
            }
            _ => {}
        }
    }

    pub fn set_time(&mut self, context: &Context, vtime: chrono::Duration) {
        let mut visited_objects = HashSet::new();

        let t = vtime.as_relative_time();
        let mbr = vtime.as_mbr();
        context.tracks_tree().search_access(&mbr, |obj_space, id| {
            let (object_id, location) = Self::location_info(
                t,
                obj_space,
                id
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

    pub fn draw_debug_info<C: Camera>(
        &mut self,
        window: &mut Window,
        camera: &mut C,
        context: &Context,
        settings: &DebugInfoSettings,
        vtime: chrono::Duration,
    ) {
        match self.rtree {
            Some(ref mut rtree) if settings.show_rtree => rtree.set_visible(true),
            Some(ref mut rtree) => rtree.set_visible(false),
            None if settings.show_rtree => self.create_rtree(context),
            _ => {}
        }

        if let Some(track_step) = settings.tracks {
            self.draw_tracks(window, context, track_step);
        }

        if settings.names {
            self.draw_names(window, camera, context, vtime);
        }
    }

    fn draw_tracks(
        &mut self,
        window: &mut Window,
        context: &Context,
        track_step: chrono::Duration
    ) {
        debug_assert!(track_step > chrono::Duration::zero());

        let mut last_pos = HashMap::new();
        let step = track_step.as_relative_time();

        let tracks_tree = context.tracks_tree().lock_obj_space();
        let tracks_mbr = tracks_tree.get_root_mbr();
        let mut begin = tracks_mbr.bounds(0).min;
        let end = tracks_mbr.bounds(0).max;

        std::mem::drop(tracks_tree);

        while begin < end {
            let mbr = mbr![t = [begin; begin]];

            context.tracks_tree().search_access(&mbr, |obj_space, id| {
                let (object_id, location) = Self::location_info(
                    begin,
                    obj_space,
                    id
                );

                match last_pos.entry(object_id) {
                    Entry::Vacant(entry) => {
                        entry.insert(location);
                    },
                    Entry::Occupied(mut entry) => {
                        let prev_location = entry.get();
                        let color= context.actor(entry.key()).object().color();

                        let from = Point3::new(
                            prev_location[0],
                            prev_location[1],
                            prev_location[2],
                        );

                        let to = Point3::new(
                            location[0],
                            location[1],
                            location[2],
                        );

                        window.draw_line(&from, &to, color);

                        entry.insert(location);
                    }
                }
            });

            begin += step;
        }
    }

    fn draw_names<C: Camera>(
        &mut self,
        window: &mut Window,
        camera: &mut C,
        context: &Context,
        vtime: chrono::Duration
    ) {
        let t = vtime.as_relative_time();
        let mbr = vtime.as_mbr();

        let hidpi = window.hidpi_factor() as f32;
        let width = window.width() as f32 * hidpi;
        let height = window.height() as f32 * hidpi;

        let screen_size = Vector2::new(width, height);

        let text_size = 85.0;
        let text_shift = Vector2::new(
            text_size * hidpi / 4.0,
            -text_size * hidpi / 2.0
        );

        context.tracks_tree().search_access(
            &mbr,
            |obj_space, id| {
                let (object_id, location) = Self::location_info(t, obj_space, id);

                let world_coord = Point3::new(
                    location[0],
                    location[1],
                    location[2],
                );

                let text_beg_location = camera.project(
                    &world_coord,
                    &screen_size
                ).scale(hidpi * 2.0);

                let text_location = text_beg_location - text_shift;
                let text_location = Point2::new(
                    text_location[0],
                    height * 2.0 - text_location[1],
                );

                let object = context.actor(&object_id).object();

                let src_color = object.color();
                let color = graphics::opposite_color(src_color);

                self.draw_text(
                    window,
                    format!("+ {}", object.name()).as_str(),
                    text_location,
                    color
                )
            }
        )
    }

    pub fn draw_text(&mut self, window: &mut Window, text: &str, pos: Point2<f32>, color: Color) {
        let scale = 75.0;
        let font = Font::default();

        window.draw_text(text, &pos, scale, &font, &color);
    }

    fn location_info(t: RelativeTime, obj_space: &TracksSpace, id: TrackPartId) -> (ObjectId, Vector) {
        let track_part_mbr = obj_space.get_data_mbr(id);
        let track_part_info = obj_space.get_data_payload(id);

        let object_id = track_part_info.object_id;

        let location = Context::location(
            track_part_mbr,
            track_part_info,
            t,
        );

        (object_id, location)
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