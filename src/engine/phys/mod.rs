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

pub type CollisionGraph = UnGraphMap<ObjectCollision, ()>;
pub type CollisionGroup = Vec<ObjectCollision>;

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

    pub fn gen_coords(&self, context: &Context, t: RelativeTime) -> (GenCoord, GenCoord) {
        let last_gen_coord = self.last_gen_coord(context);

        let step = t.as_absolute_time() - last_gen_coord.time();
        (
            last_gen_coord.clone(),
            next_gen_coord(&last_gen_coord, step)
        )
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
    graph: &CollisionGraph,
    mut group: CollisionGroup
) -> (RelativeTime, CollisionGraph) {
    let mut min_collision_time = RelativeTime::INFINITY;

    let mut collision_graph = CollisionGraph::new();
    let mut handled = HashSet::new();

    while let Some(lhs) = group.pop() {
        for rhs in graph.neighbors(lhs) {
            let pair = CollisionPair(lhs.object_id, rhs.object_id);

            if handled.contains(&pair) {
                continue;
            }

            let obj_space = context.tracks_tree().lock_obj_space();

            let lhs_radius = context.actor(&lhs.object_id).object().radius();
            let rhs_radius = context.actor(&rhs.object_id).object().radius();
            let radius_sum = lhs_radius + rhs_radius;

            let lhs_track_part = obj_space.get_data_payload(lhs.track_part_id);
            let rhs_track_part = obj_space.get_data_payload(rhs.track_part_id);

            let lhs_mbr = obj_space.get_data_mbr(lhs.track_part_id);
            let rhs_mbr = obj_space.get_data_mbr(rhs.track_part_id);

            let lhs_bounds = lhs_mbr.bounds(0);
            let rhs_bounds = rhs_mbr.bounds(0);

            let valid_range = Range {
                start: lhs_bounds.min.max(rhs_bounds.min),
                end: lhs_bounds.max.min(rhs_bounds.max)
            };

            let collision_time = math::ranged_secant(
                valid_range,
                EPS,
                |t| {
                    let lhs_location = Context::location(
                        lhs_mbr,
                        lhs_track_part,
                        t
                    );

                    let rhs_location = Context::location(
                        rhs_mbr,
                        rhs_track_part,
                        t
                    );

                    let distance = (rhs_location - lhs_location).norm() - radius_sum;
                    distance
                }
            );

            if let Some(collision_time) = collision_time {
                if collision_time < min_collision_time {
                    min_collision_time = collision_time;

                    collision_graph.clear();
                }

                if abs_diff_eq![collision_time, min_collision_time, epsilon = EPS] {
                    collision_graph.add_edge(
                        lhs,
                        rhs,
                        ()
                    );
                }
            }

            handled.insert(pair);
        }
    }

    (min_collision_time, collision_graph)
}

pub fn compute_collisions(context: &Context, t: RelativeTime, graph: CollisionGraph) {
    let mut gen_coords = HashMap::new();
    let mut collision_ids = HashSet::new();

    for lhs in graph.nodes() {
        collision_ids.insert(lhs.object_id);

        let (partners_mass, partners_impulses) = graph.neighbors(lhs)
            .map(|rhs| {
                let mass = context.actor(&rhs.object_id).object().mass();

                let src_gen_coord: &GenCoord;
                match gen_coords.entry(rhs.object_id) {
                    Entry::Vacant(entry) => {
                        let ((_, gen_coord), _after_col_vel) = entry.insert(
                            (rhs.gen_coords(&context, t), Vector::zeros())
                        );

                        src_gen_coord = gen_coord;
                    }
                    Entry::Occupied(entry) => {
                        let (
                            (_last_gen_coord, gen_coord),
                            _after_col_vel
                        ) = entry.into_mut();

                        src_gen_coord = gen_coord;
                    }
                }

                (mass, src_gen_coord.velocity().scale(mass))
            })
            .fold(
                (0.0, Vector::zeros()),
                |(acc_mass, acc_impulse), (mass, impulse)| {
                    (acc_mass + mass, acc_impulse + impulse)
                }
            );

        let mass = context.actor(&lhs.object_id).object().mass();

        match gen_coords.entry(lhs.object_id) {
            Entry::Vacant(entry) => {
                let gen_coords = lhs.gen_coords(&context, t);

                let after_collision_velocity = compute_collision_velocity(
                    mass,
                    partners_mass,
                    gen_coords.1.velocity(),
                    partners_impulses
                );

                entry.insert((gen_coords, after_collision_velocity));
            },
            Entry::Occupied(mut entry) => {
                let (gen_coords, after_collision_velocity) = entry.get_mut();

                *after_collision_velocity = compute_collision_velocity(
                    mass,
                    partners_mass,
                    gen_coords.1.velocity(),
                    partners_impulses
                );
            }
        }
    }

    context.cancel_tracks_except(t, collision_ids);

    for (&object_id, ((lhs_coord, rhs_coord), final_velocity)) in gen_coords.iter() {
        let track_part = TrackPartInfo::new(
            object_id,
            lhs_coord,
            rhs_coord,
            Some(final_velocity.clone())
        );

        let actor = context.actor(&object_id);
        let radius = actor.object().radius();

        let time_range = TimeRange::with_bounds(lhs_coord.time(), rhs_coord.time());

        let mbr = make_track_part_mbr(
            &time_range,
            radius,
            &track_part
        );

        context.tracks_tree().insert(track_part, mbr);

        let last_gen_coord = GenCoord::new(
            rhs_coord.time(),
            rhs_coord.location().clone(),
            final_velocity.clone()
        );

        actor.set_last_gen_coord(last_gen_coord);
    }
}

fn compute_collision_velocity(mass: Mass, partners_mass: Mass, src_velocity: &Vector, partners_impulses: Vector) -> Vector {
    let total_mass = mass + partners_mass;

    (src_velocity.scale(mass - partners_mass) + partners_impulses.scale(2.0)).unscale(total_mass)
}