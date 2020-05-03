use {
    std::sync::{Arc, Mutex},
    lazy_static::lazy_static,
    rusqlite::{params, Connection, NO_PARAMS},
    threadpool::ThreadPool,
    log::info,
    crate::{
        make_error, 
        scene::{
            Object4d,
            Attractor,
            track::{
                Track,
                TrackNode,
                TrackAtom,
            }
        },
        r#type::{
            Vector,
            Mass,
            TimeFormat,
        },
        shared::Shared,
        Result
    },
};

const LOG_TARGET: &'static str = "physics";

pub struct Engine {
    /// Occupied spaces DB
    osdb: Connection,
    compute_pool: ThreadPool,
    update_relative_time: f32,
}

impl Engine {
    pub fn new() -> Result<Self> {
        let engine = Self {
            osdb: Connection::open_in_memory()
                .map_err(|err| make_error![Error::Physics::Init(err)])?,
            compute_pool: ThreadPool::new(num_cpus::get()),
            update_relative_time: 0.25,
        };

        Ok(engine)
    }

    pub fn lower_update_time_threshold(&self) -> f32 {
        self.update_relative_time
    }

    pub fn upper_update_time_threshold(&self) -> f32 {
        1.0 - self.update_relative_time
    }

    pub fn track_relative_time(&self, object: &Object4d, vtime: &chrono::Duration) -> f32 {
        assert!((0.0..1.0).contains(&self.update_relative_time));

        let track = object.track();
        let track_time_start = track.time_start();
        let computed_duration = (track.time_end() - track_time_start).num_milliseconds() as f32;
        let offset = (*vtime - track_time_start).num_milliseconds() as f32;
        let relative_time = offset / computed_duration;

        relative_time
    }

    pub fn update_objects(
        &mut self, 
        vtime: &chrono::Duration, 
        mut objects: Vec<Arc<Mutex<Object4d>>>, 
        attractors: Vec<Arc<Attractor>>
    ) {
        // TODO load from DB

        macro_rules! log_update {
            ($id:expr, $from:expr => $to:ident) => {
                info! {
                    target: LOG_TARGET,
                    "`{}`: update track from {} to /{}/",
                    $id,
                    TimeFormat::VirtualTimeShort($from),
                    stringify![$to]
                }
            };
        }

        let mut compute_direction = TimeDirection::Forward;
        let mut uncomputed_objects = vec![];

        while let Some(object) = objects.pop() {
            let rt = self.track_relative_time(
                &*object.lock().unwrap(), 
                vtime
            );

            if rt.is_nan() || rt < self.lower_update_time_threshold() || rt > self.upper_update_time_threshold() {
                let mut sync_object = object.lock().unwrap();
                let track = sync_object.track_mut();
                let half_time = track.time_start() + (track.time_end() - track.time_start()) / 2;

                if rt.is_nan() || rt > self.upper_update_time_threshold() {
                    compute_direction = TimeDirection::Forward;
                    track.truncate(..half_time);

                    log_update!(sync_object.id(), half_time => future);
                }  else {
                    compute_direction = TimeDirection::Backward;
                    track.truncate(half_time..);

                    log_update!(sync_object.id(), half_time => past);
                }

                std::mem::drop(sync_object);
                uncomputed_objects.push(object);
            }
        }

        self.compute(compute_direction, uncomputed_objects, attractors);
    }

    pub fn compute(
        &mut self, 
        compute_time_direction: TimeDirection,
        mut objects: Vec<Arc<Mutex<Object4d>>>,
        attractors: Vec<Arc<Attractor>>,
    ) {
        // TODO compute collisions after each step

        let add_node: fn(&mut Track, TrackNode); 
        let last_node: fn(&Track) -> Shared<TrackNode>; 
        let last_atom: for<'n> fn(&'n TrackNode) -> &'n TrackAtom;
        
        match compute_time_direction {
            TimeDirection::Forward => {
                add_node = Track::push_back;
                last_node = Track::node_end;
                last_atom = TrackNode::atom_end;
            },
            TimeDirection::Backward => {
                add_node = Track::push_front;
                last_node = Track::node_start;
                last_atom = TrackNode::atom_start;
            },
        }

        let mut temp_vec = vec![];

        let mut objects = &mut objects;
        let mut remaining = &mut temp_vec;

        while !objects.is_empty() {
            while let Some(object) = objects.pop() {
                let sync_object = object.lock().unwrap();
                if !sync_object.track().is_fully_computed() {
                    remaining.push(Arc::clone(&object));

                    let object = Arc::clone(&object);
                    let attractors = attractors.clone();
                    self.compute_pool.execute(move || {
                        let mut sync_object = object.lock().unwrap();
                        let obj_mass = sync_object.mass();
                        let track = sync_object.track_mut();

                        let node = last_node(track);
                        let node = node.read().unwrap();

                        let atom = last_atom(&*node);
                        let step = match compute_time_direction { 
                            TimeDirection::Forward => track.relative_compute_step(),
                            TimeDirection::Backward => -track.relative_compute_step(),
                        };

                        let mut new_atom = atom.at_next_location(step);

                        // TODO: Check collisions
                        Self::make_new_atom(
                            obj_mass, 
                            &mut new_atom, 
                            step, 
                            attractors
                        ).unwrap();

                        add_node(track, new_atom.into());
                    });
                }
            }

            std::mem::swap(&mut objects, &mut remaining);
            self.compute_pool.join();
        }
    }

    pub fn make_new_atom(obj_mass: Mass, atom: &mut TrackAtom, step: f32, attractors: Vec<Arc<Attractor>>) -> Result<()> {
        // TODO CHECK FOR COLLISION

        let mut acceleration = compute_acceleration(obj_mass, atom.location(), &attractors);
        acceleration.scale_mut(step);
        atom.set_velocity(atom.velocity() + acceleration);

        Ok(())
    }
}

fn compute_acceleration(obj_mass: Mass, location: &Vector, attractors: &Vec<Arc<Attractor>>) -> Vector {
    let mut acceleration = Vector::zeros();
    for attractor in attractors.iter() {
        let mut dir = attractor.location() - *location;
        let distance = dir.norm();

        dir.unscale_mut(distance);

        let distance2 = distance * distance;
        let attr_mass = attractor.mass();
        let gravity_coeff = attractor.gravity_coeff();
        
        let mut attr_acceleration = dir;
        attr_acceleration.scale_mut(gravity_coeff * attr_mass);
        attr_acceleration.unscale_mut(obj_mass * distance2);

        acceleration += attr_acceleration;
    }

    acceleration
}

#[derive(Clone, Copy)]
pub enum TimeDirection {
    Forward,
    Backward,
}
