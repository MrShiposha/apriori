use {
    std::{
        collections::{HashMap, hash_map::Entry},
        cmp::Ordering,
        hash::{Hash, Hasher},
        sync::{Arc, RwLock}
    },
    crate::{
        r#type::{ObjectId, Vector, AsAbsoluteTime, RelativeTime, Coord},
        object::GenCoord,
        engine::{
            context::{Context, TrackPartId, TrackPartInfo, TracksSpace},
        },
    },
    petgraph::graphmap::UnGraphMap,
    lr_tree::{LRTree, InsertHandler},
};

pub type TrackPartIdx = usize;
pub type CollisionGraph = UnGraphMap<ObjectCollision, ()>;
pub type PossibleCollisionsGraph = UnGraphMap<ObjectId, ()>;
pub type PossibleCollisionsGroup = Vec<ObjectId>;

pub struct CollisionPair(pub ObjectId, pub ObjectId);

impl Hash for CollisionPair {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let CollisionPair(mut lhs, mut rhs) = self;

        if lhs > rhs {
            std::mem::swap(&mut lhs, &mut rhs);
        }

        lhs.hash(state);
        rhs.hash(state);
    }
}

impl PartialEq for CollisionPair {
    fn eq(&self, other: &Self) -> bool {
        let strait_eq = self.0 == other.0 && self.1 == other.1;
        let reverse_eq = self.0 == other.1 && self.1 == other.0;

        strait_eq || reverse_eq
    }
}

impl Eq for CollisionPair {}

#[derive(Clone, Copy)]
pub struct ObjectCollision {
    pub object_id: ObjectId,
    pub track_part_id: TrackPartId,
}

impl ObjectCollision {
    pub fn new(object_id: ObjectId, track_part_id: TrackPartId) -> Self {
        Self {
            object_id,
            track_part_id
        }
    }

    pub fn path(&self, context: &Context, t: RelativeTime) -> CollidingGenCoords {
        let last_gen_coord = self.last_gen_coord(context);

        let step = t.as_absolute_time() - last_gen_coord.time();
        CollidingGenCoords {
            start: last_gen_coord.clone(),
            end: super::next_gen_coord(&last_gen_coord, step)
        }
    }

    fn last_gen_coord(&self, context: &Context) -> GenCoord {
        let obj_space = context.tracks_tree().lock_obj_space();

        let mbr = obj_space.get_data_mbr(self.track_part_id);
        let track_part_info = obj_space.get_data_payload(self.track_part_id);

        let last_t = mbr.bounds(0).min.as_absolute_time();
        GenCoord::new(
            last_t,
            track_part_info.start_location,
            track_part_info.start_velocity
        )
    }
}

impl PartialEq for ObjectCollision {
    fn eq(&self, other: &Self) -> bool {
        self.object_id.eq(&other.object_id)
    }
}

impl Eq for ObjectCollision {}

impl PartialOrd for ObjectCollision {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.object_id.partial_cmp(&other.object_id)
    }
}

impl Ord for ObjectCollision {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.object_id.cmp(&other.object_id)
    }
}

impl Hash for ObjectCollision {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.object_id.hash(state);
    }
}

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