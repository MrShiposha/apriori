use {
    crate::{
        engine::{math, context::{Context, TrackPartId, TrackPartInfo, db_util::make_track_part_mbr, TimeRange}},
        object::{GenCoord},
        r#type::{ObjectId, Vector, Mass, AsRelativeTime, AsAbsoluteTime, RelativeTime}
    },
    std::{
        sync::Arc,
        ops::Range,
        hash::{Hash, Hasher},
        collections::{HashSet, HashMap, hash_map::Entry},
    },
    petgraph::{graphmap::UnGraphMap},
    approx::abs_diff_eq
};

pub mod collision;

pub type CollisionGraph = UnGraphMap<ObjectCollision, ()>;
// pub type CollisionGroup = Vec<ObjectCollision>;

const EPS: f32 = 0.0001;
pub struct CollisionPair(ObjectId, ObjectId);

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
    object_id: ObjectId,
    track_part_id: TrackPartId,
}

impl ObjectCollision {
    pub fn new(object_id: ObjectId, track_part_id: TrackPartId) -> Self {
        Self {
            object_id,
            track_part_id
        }
    }

    pub fn path(&self, context: &Context, t: RelativeTime) -> collision::CollidingGenCoords {
        let last_gen_coord = self.last_gen_coord(context);

        let step = t.as_absolute_time() - last_gen_coord.time();
        collision::CollidingGenCoords {
            start: last_gen_coord.clone(),
            end: next_gen_coord(&last_gen_coord, step)
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

pub fn acceleration(location: &Vector, velocity: &Vector) -> Vector {
    let rvec = location.norm();

    let limit = 30.0;

    if rvec > limit {
        -location.normalize().scale(velocity.norm() * 0.20)
    } else if rvec > 0.0 {
        location.normalize().scale(velocity.norm() * 0.25)
    } else {
        Vector::zeros()
    }
}

pub fn next_gen_coord(last_gen_coord: &GenCoord, step: chrono::Duration) -> GenCoord {
    let rel_step = step.as_relative_time() / 2.0;

    let src_location = last_gen_coord.location();
    let src_velocity = last_gen_coord.velocity();

    let (temp_location, temp_velocity) = next_gen_coord_helper(
        src_location,
        src_velocity,
        rel_step,
    );

    let (new_location, new_velocity) = next_gen_coord_helper(
        &temp_location,
        &temp_velocity,
        rel_step,
    );

    let new_time = last_gen_coord.time() + step;

    GenCoord::new(new_time, new_location, new_velocity)
}

fn next_gen_coord_helper(location: &Vector, velocity: &Vector, step: RelativeTime) -> (Vector, Vector) {
    let a = acceleration(location, velocity);
    let new_velocity = velocity + a.scale(step);

    let new_location = location + velocity.scale(step);

    (new_location, new_velocity)
}

pub fn find_collision_group(
    context: &Context,
    checker: &collision::CollisionChecker,
    mut group: collision::PossibleCollisionsGroup
) -> (RelativeTime, CollisionGraph) {
    let mut min_collision_time = RelativeTime::INFINITY;

    let mut collision_graph = CollisionGraph::new();
    let mut handled = HashSet::new();

    let graph = checker.collisions();

    while let Some(lhs) = group.pop() {
        for rhs in graph.neighbors(lhs) {
            let pair = CollisionPair(lhs, rhs);

            if handled.contains(&pair) {
                continue;
            }

            let obj_space = context.tracks_tree().lock_obj_space();

            let lhs_radius = context.actor(&lhs).object().radius();
            let rhs_radius = context.actor(&rhs).object().radius();
            let radius_sum = lhs_radius + rhs_radius;

            // let lhs_track_part = obj_space.get_data_payload(lhs.track_part_id);
            // let rhs_track_part = obj_space.get_data_payload(rhs.track_part_id);

            // let lhs_mbr = obj_space.get_data_mbr(lhs.track_part_id);
            // let rhs_mbr = obj_space.get_data_mbr(rhs.track_part_id);

            // let lhs_bounds = lhs_mbr.bounds(0);
            // let rhs_bounds = rhs_mbr.bounds(0);

            let lhs_path = checker.path(lhs);
            let rhs_path = checker.path(rhs);

            let valid_range = Range {
                start: lhs_path.min_t().max(rhs_path.min_t()),
                end: lhs_path.max_t().min(rhs_path.max_t())
            };

            let collision_time = math::find_root(
                valid_range,
                EPS,
                |t| {
                    // let lhs_location = Context::location(
                    //     lhs_mbr,
                    //     lhs_track_part,
                    //     t
                    // );

                    // let rhs_location = Context::location(
                    //     rhs_mbr,
                    //     rhs_track_part,
                    //     t
                    // );

                    let obj_space = &*obj_space;

                    let lhs_part_idx = lhs_path.track_part_idx(obj_space, t);
                    let rhs_part_idx = rhs_path.track_part_idx(obj_space, t);

                    let lhs_location = lhs_path.location(obj_space, lhs_part_idx, t);
                    let rhs_location = rhs_path.location(obj_space, rhs_part_idx, t);

                    let distance = (rhs_location - lhs_location).norm() - radius_sum;
                    distance
                }
            );

            if let Some(collision_time) = collision_time {
                if collision_time < min_collision_time {
                    min_collision_time = collision_time;

                    collision_graph.clear();
                }

                let lhs_part_id = lhs_path.track_part_id(&*obj_space, collision_time);
                let rhs_part_id = rhs_path.track_part_id(&*obj_space, collision_time);

                if abs_diff_eq![collision_time, min_collision_time, epsilon = EPS] {
                    collision_graph.add_edge(
                        ObjectCollision::new(lhs, lhs_part_id),
                        ObjectCollision::new(rhs, rhs_part_id),
                        ()
                    );
                }
            }

            handled.insert(pair);
        }
    }

    (min_collision_time, collision_graph)
}

/// Returns all ids of objects with canceled tracks.
pub fn compute_collisions(context: &Context, t: RelativeTime, graph: CollisionGraph) -> HashSet<ObjectId> {
    let mut collision_vectors = collision::CollisionVectors::new();
    let mut collision_ids = HashSet::new();

    for lhs in graph.nodes() {
        collision_ids.insert(lhs.object_id);
        let lhs_path = collision_vectors.object_path(context, &lhs, t);

        let (partners_mass, partners_impulses, collision_dir) = graph.neighbors(lhs)
            .map(|rhs| {
                let mass = context.actor(&rhs.object_id).object().mass();

                let rhs_path = collision_vectors.object_path(context, &rhs, t);
                let collision_dir = (lhs_path.end.location() - rhs_path.end.location()).normalize();
                let normal_velocity = collision_dir.scale(
                    rhs_path.end.velocity().dot(&collision_dir)
                );

                (mass, normal_velocity.scale(mass), collision_dir)
            })
            .fold(
                (0.0, Vector::zeros(), Vector::zeros()),
                |(acc_mass, acc_impulse, acc_col_dir), (mass, impulse, dir)| {
                    (acc_mass + mass, acc_impulse + impulse, acc_col_dir + dir)
                }
            );

        let mass = context.actor(&lhs.object_id).object().mass();
        let src_velocity = lhs_path.end.velocity().clone();

        let collision_dir = collision_dir.normalize();
        let normal_velocity = collision_dir.scale(
            src_velocity.dot(&collision_dir)
        );
        let tangent_velocity = src_velocity - normal_velocity;

        let final_normal_velocity = compute_central_collision_velocity(
            mass,
            partners_mass,
            &normal_velocity,
            partners_impulses
        );

        let final_velocity = final_normal_velocity + tangent_velocity;

        collision_vectors.set_final_velocity(context, &lhs, t, final_velocity);
    }

    let canceled_objects_ids = context.cancel_tracks_except(t, collision_ids);

    let collision_vectors = collision_vectors.into_iter();
    for (object_id, (path, final_velocity)) in collision_vectors  {
        let collision::CollidingGenCoords {
            start,
            end
        } = path;

        let track_part = TrackPartInfo::new(
            object_id,
            &start,
            &end,
            Some(final_velocity.clone())
        );

        let actor = context.actor(&object_id);
        let radius = actor.object().radius();

        let time_range = TimeRange::with_bounds(start.time(), end.time());

        let mbr = make_track_part_mbr(
            &time_range,
            radius,
            &track_part
        );

        context.tracks_tree().insert(track_part, mbr);

        let last_gen_coord = GenCoord::new(
            end.time(),
            end.location().clone(),
            final_velocity.clone()
        );

        actor.set_last_gen_coord(last_gen_coord);
    }

    canceled_objects_ids
}

fn compute_central_collision_velocity(
    mass: Mass,
    partners_mass: Mass,
    src_velocity: &Vector,
    partners_impulses: Vector
) -> Vector {
    let total_mass = mass + partners_mass;

    (src_velocity.scale(mass - partners_mass) + partners_impulses.scale(2.0)).unscale(total_mass)
}