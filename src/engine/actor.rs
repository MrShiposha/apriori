use {
    lr_tree::*,
    crate::{
        make_error,
        Result,
        object::{Object, GenCoord},
        r#type::{ObjectId, Vector, Coord, AsRelativeTime, AsAbsoluteTime},
        engine::{
            math,
            context::{TimeRange, GlobalTrackPartId},
        }
    },
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
    last_location: Option<GenCoord>,
}

impl Actor {
    pub fn new(
        object: Object,
        track_parts_space: TrackPartsSpace,
    ) -> Self {
        Self {
            object,
            track_parts_tree: TrackPartsTree::with_obj_space(track_parts_space),
            last_location: None,
        }
    }

    pub fn location(&self, vtime: chrono::Duration) -> Result<Vector> {
        let t = vtime.as_relative_time();

        let obj_space = self.track_parts_tree().lock_obj_space();
        let all_computed_time = obj_space.get_root_mbr();

        if all_computed_time.is_undefined() {
            Err(make_error!(Error::Interpolation::NoTrackParts))
        }
        else if t > all_computed_time.bounds(0).max {
            Err(make_error!(Error::Interpolation::ObjectIsNotComputed(
                self.last_gen_coord().unwrap().location().clone()
            )))
        } else if t < all_computed_time.bounds(0).min {
            Err(make_error!(Error::Interpolation::FutureObject))
        } else {
            std::mem::drop(obj_space);

            let mbr = mbr![t = [t; t]];

            let mut location = Vector::zeros();
            self.track_parts_tree.search_access(
                &mbr,
                |obj_space, id| {
                    let time = obj_space.get_data_mbr(id);
                    let track_part_info = obj_space.get_data_payload(id);

                    location = math::hermite_interpolation(
                        &track_part_info.start_location,
                        &track_part_info.start_velocity,
                        time.bounds(0).min,
                        &track_part_info.end_location,
                        &track_part_info.end_velocity,
                        time.bounds(0).max,
                        t
                    );
                }
            );

            Ok(location)
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

        track_part_id
    }

    pub fn track_parts_tree(&self) -> &TrackPartsTree {
        &self.track_parts_tree
    }

    pub fn set_last_location(&mut self, last_location: GenCoord) {
        self.last_location = Some(last_location);
    }

    pub fn last_gen_coord(&self) -> Option<GenCoord> {
        self.last_location.clone()
    }
}

impl Clone for Actor {
    fn clone(&self) -> Self {
        let object = self.object.clone();
        let track_parts_space = self.track_parts_tree.lock_obj_space().clone_shrinked();
        let new_tree = LRTree::with_obj_space(track_parts_space);

        Actor {
            object,
            track_parts_tree: new_tree,
            last_location: None,
        }
    }
}
