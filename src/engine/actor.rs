use {
    lr_tree::*,
    crate::{
        object::{Object, GenCoord},
        r#type::{ObjectId, Vector, Coord, AsRelativeTime, AsAbsoluteTime},
        engine::context::{TimeRange, GlobalTrackPartId},
    },
    std::{
        sync::atomic::{AtomicUsize, Ordering},
    }
};

pub type TrackPartId = NodeId;

#[derive(Debug, Clone)]
pub struct TrackPartInfo {
    pub global_track_part_id: GlobalTrackPartId,
    pub start_location: Vector,
    pub end_location: Vector,
    pub start_velocity: Vector,
    pub end_velocity: Vector,
    pub collision_info: Option<CollisionInfo>,
}

#[derive(Debug, Clone)]
pub struct CollisionInfo {
    pub final_velocity: Vector,
    pub partners_ids: Vec<(ObjectId, TrackPartId)>,
}

pub type TrackPartsTree = LRTree<Coord, TrackPartInfo>;
pub type TrackPartsSpace = ObjSpace<Coord, TrackPartInfo>;

pub struct Actor {
    object: Object,
    track_parts_tree: TrackPartsTree,
    initial_location: Option<GenCoord>,
    last_track_part_id: AtomicUsize,
}

impl Actor {
    pub fn new(
        object: Object,
        track_parts_space: TrackPartsSpace,
    ) -> Self {
        Self {
            object,
            track_parts_tree: TrackPartsTree::with_obj_space(track_parts_space),
            initial_location: None,
            last_track_part_id: AtomicUsize::default(),
        }
    }

    pub fn object(&self) -> &Object {
        &self.object
    }

    pub fn add_track_part_unchecked(
        &self,
        time_range: &TimeRange,
        track_part_info: TrackPartInfo
    ) -> TrackPartId {
        let from = time_range.start().as_relative_time();
        let to = time_range.end().as_relative_time();

        let track_part_id = self.track_parts_tree
            .lock_obj_space_write()
            .make_data_node(
                track_part_info,
                mbr![t = [from; to]]
            );

        self.last_track_part_id.store(track_part_id, Ordering::SeqCst);

        track_part_id
    }

    pub fn track_parts_tree(&self) -> &TrackPartsTree {
        &self.track_parts_tree
    }

    pub fn set_initial_location(&mut self, initial_location: GenCoord) {
        self.initial_location = Some(initial_location);
    }

    pub fn clear_initial_location(&mut self) {
        self.initial_location = None;
    }

    pub fn last_gen_coord(&self) -> Option<GenCoord> {
        let mut coord = self.initial_location.clone();

        if coord.is_none() && !self.track_parts_tree.lock_obj_space().is_empty() {
            self.track_parts_tree.access_object(
                self.last_track_part_id.load(Ordering::SeqCst),
                |part_info, mbr| {
                    let t = mbr.bounds(0).max.as_absolute_time();
                    let location = part_info.end_location;

                    let velocity = part_info.collision_info
                        .as_ref()
                        .map(|info| info.final_velocity)
                        .unwrap_or(part_info.end_velocity);

                    coord = Some(GenCoord::new(t, location, velocity))
                }
            );
        }

        coord
    }
}

impl Clone for Actor {
    fn clone(&self) -> Self {
        let object = self.object.clone();
        let initial_location = self.initial_location.clone();
        let track_parts_space = self.track_parts_tree.lock_obj_space().clone_shrinked();

        let new_tree = LRTree::with_obj_space(track_parts_space);
        let alpha = 0.45;
        new_tree.rebuild(alpha);

        let last_track_part_id = self.last_track_part_id.load(Ordering::SeqCst);

        Actor {
            object,
            track_parts_tree: new_tree,
            initial_location,
            last_track_part_id: AtomicUsize::new(last_track_part_id),
        }
    }
}