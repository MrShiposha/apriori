use {
    std::{
        collections::{HashMap, hash_map::Entry},
        cmp::Ordering,
        sync::{Arc, RwLock}
    },
    crate::{
        r#type::{ObjectId, Vector, RelativeTime, Coord},
        object::GenCoord,
        engine::{
            context::{Context, TrackPartId, TrackPartInfo, TracksSpace},
            phys::ObjectCollision,
        },
    },
    petgraph::graphmap::UnGraphMap,
    lr_tree::{LRTree, InsertHandler},
};

pub type TrackPartIdx = usize;
pub type PossibleCollisionsGraph = UnGraphMap<ObjectId, ()>;
pub type PossibleCollisionsGroup = Vec<ObjectId>;

#[derive(Debug)]
pub struct CollisionChecker {
    collisions: PossibleCollisionsGraph,
    collision_paths: HashMap<ObjectId, CollidingPath>,
}

impl CollisionChecker {
    pub fn new() -> Self {
        Self {
            collisions: PossibleCollisionsGraph::new(),
            collision_paths: HashMap::new(),
        }
    }

    pub fn clear(&mut self) {
        self.collisions.clear();
        self.collision_paths.clear();
    }

    pub fn collisions(&self) -> &PossibleCollisionsGraph {
        &self.collisions
    }

    pub fn path(&self, object_id: ObjectId) -> &CollidingPath {
        self.collision_paths.get(&object_id).unwrap()
    }

    fn add_path(&mut self, space: &TracksSpace, object_id: ObjectId, track_part_id: TrackPartId) {
        match self.collision_paths.entry(object_id) {
            Entry::Vacant(entry) => {
                let mut path = CollidingPath::new();
                path.add_track_part(space, track_part_id);

                entry.insert(path);
            }
            Entry::Occupied(mut entry) => {
                entry.get_mut().add_track_part(space, track_part_id);
            }
        }
    }

    fn add_edge(&mut self, lhs: ObjectId, rhs: ObjectId) {
        self.collisions.add_edge(lhs, rhs, ());
    }
}

impl InsertHandler<Coord, TrackPartInfo> for Arc<RwLock<CollisionChecker>> {
    fn before_insert(&mut self, space: &TracksSpace, id: TrackPartId) {
        let object_id = space.get_data_payload(id).object_id;
        let mbr = space.get_data_mbr(id);

        self.write().unwrap().add_path(space, object_id, id);

        LRTree::search_access_obj_space(
            space,
            mbr,
            |obj_space, partner_track_id| {
                let partner_id = obj_space.get_data_payload(partner_track_id).object_id;

                if obj_space.is_removed(&partner_track_id)
                || partner_id == object_id {
                    return;
                }

                let mut write = self.write().unwrap();
                write.add_path(space, partner_id, partner_track_id);
                write.add_edge(object_id, partner_id);
            }
        );
    }
}

#[derive(Debug)]
pub struct CollidingPath {
    track_part_ids: Vec<TrackPartId>,
    min_t: RelativeTime,
    max_t: RelativeTime
}

impl CollidingPath {
    pub fn new() -> Self {
        Self {
            track_part_ids: vec![],
            min_t: RelativeTime::INFINITY,
            max_t: RelativeTime::NEG_INFINITY,
        }
    }

    pub fn min_t(&self) -> RelativeTime {
        self.min_t
    }

    pub fn max_t(&self) -> RelativeTime {
        self.max_t
    }

    fn add_track_part(&mut self, space: &TracksSpace, track_part_id: TrackPartId) {
        let new_t = space.get_data_mbr(track_part_id).bounds(0).min;
        let new_max_t = space.get_data_mbr(track_part_id).bounds(0).max;

        if new_t < self.min_t {
            self.min_t = new_t;
        }

        if new_max_t > self.max_t {
            self.max_t = new_max_t;
        }

        match self.track_part_ids.binary_search_by(|&item| {
            let item_t = space.get_data_mbr(item).bounds(0).min;

            item_t.partial_cmp(&new_t).unwrap()
        }) {
            Err(idx) => self.track_part_ids.insert(idx, track_part_id),
            Ok(_) => {}
        }
    }

    pub fn track_part_idx(&self, space: &TracksSpace, t: RelativeTime) -> TrackPartIdx {
        debug_assert!(self.min_t <= t && t <= self.max_t);

        match self.track_part_ids.binary_search_by(|&item| {
            let bounds = space.get_data_mbr(item).bounds(0);

            if bounds.is_in_bound(&t) {
                Ordering::Equal
            } else if bounds.max < t {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        }) {
            Ok(idx) => idx,
            Err(_) => unreachable!()
        }
    }

    pub fn track_part_id(&self, space: &TracksSpace, t: RelativeTime) -> TrackPartId {
        let idx = self.track_part_idx(space, t);

        self.track_part_ids[idx]
    }

    pub fn location(&self, space: &TracksSpace, track_part_idx: TrackPartIdx, t: RelativeTime) -> Vector {
        let id = self.track_part_ids[track_part_idx];

        let mbr = space.get_data_mbr(id);
        let track_part_info = space.get_data_payload(id);

        Context::location(mbr, track_part_info, t)
    }
}

#[derive(Clone)]
pub struct CollidingGenCoords {
    pub start: GenCoord,
    pub end: GenCoord,
}

pub struct CollisionVectors {
    vectors: HashMap<ObjectId, (CollidingGenCoords, Vector)>
}

impl CollisionVectors {
    pub fn new() -> Self {
        Self {
            vectors: HashMap::new()
        }
    }

    pub fn object_path(&mut self, context: &Context, colliding_object: &ObjectCollision, t: RelativeTime) -> CollidingGenCoords {
        match self.vectors.entry(colliding_object.object_id) {
            Entry::Vacant(entry) => {
                let (path, _after_col_vel) = entry.insert(
                    (colliding_object.path(context, t), Vector::zeros())
                );

                path.clone()
            }
            Entry::Occupied(entry) => {
                let (
                    path,
                    _after_col_vel
                ) = entry.into_mut();

                path.clone()
            }
        }
    }

    pub fn set_final_velocity(
        &mut self,
        context: &Context,
        colliding_object: &ObjectCollision,
        t: RelativeTime,
        final_velocity: Vector
    ) {
        match self.vectors.entry(colliding_object.object_id) {
            Entry::Vacant(entry) => {
                let (_path, _after_col_vel) = entry.insert(
                    (colliding_object.path(context, t), final_velocity)
                );
            }
            Entry::Occupied(entry) => {
                let (
                    _path,
                    after_col_vel
                ) = entry.into_mut();

                *after_col_vel = final_velocity;
            }
        }
    }
}

impl IntoIterator for CollisionVectors {
    type Item = (ObjectId, (CollidingGenCoords, Vector));

    type IntoIter = <HashMap<ObjectId, (CollidingGenCoords, Vector)> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.vectors.into_iter()
    }
}