use {
    lr_tree::*,
    crate::{
        object::{Object, GenCoord},
        r#type::{ObjectId, Vector, Coord}
    },
    std::mem::MaybeUninit,
};

pub type TrackPartId = NodeId;

#[derive(Debug, Clone)]
pub struct TrackPartInfo {
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
    last_id: NodeId,
}

impl Actor {
    pub fn new(
        object: Object,
        track_parts_space: TrackPartsSpace,
        initial_location: Option<GenCoord>,
    ) -> Self {
        Self {
            object,
            track_parts_tree: TrackPartsTree::with_obj_space(track_parts_space),
            initial_location,
            last_id: NodeId::default(),
        }
    }

    pub fn track_parts_tree(&self) -> &TrackPartsTree {
        &self.track_parts_tree
    }

    pub fn last_gen_coord(&self) -> GenCoord {
        let mut coord = MaybeUninit::<GenCoord>::uninit();

        self.track_parts_tree.access_object(
            self.last_id,
            |part_info, mbr| {
                let t = mbr.bounds(0).max;
                let location = part_info.end_location;

                let velocity = part_info.collision_info
                    .as_ref()
                    .map(|info| info.final_velocity)
                    .unwrap_or(part_info.end_velocity);

                unsafe {
                    coord.as_mut_ptr().write(GenCoord::new(t, location, velocity));
                }
            }
        );

        unsafe {
            coord.assume_init()
        }
    }
}

impl Clone for Actor {
    fn clone(&self) -> Self {
        let object = self.object.clone();
        let initial_location = self.initial_location.clone();
        let track_parts_space = self.track_parts_tree.lock_obj_space().clone_shrinked();

        let is_space_empty = track_parts_space.is_empty();

        let new_tree = LRTree::with_obj_space(track_parts_space);

        let last_id;
        if initial_location.is_some() {
            last_id = NodeId::default();
        } else {
            assert!(!is_space_empty);

            let max_time = new_tree.lock_obj_space().get_root_mbr().bounds(0).max;
            last_id = *new_tree.search(&mbr![t = [max_time; max_time]]).first().unwrap();
        }

        Actor {
            object,
            track_parts_tree: new_tree,
            initial_location,
            last_id,
        }
    }
}