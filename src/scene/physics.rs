use {
    std::sync::{Arc, Mutex},
    lazy_static::lazy_static,
    rusqlite::{params, Connection, NO_PARAMS},
    threadpool::ThreadPool,
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
        },
        shared::Shared,
        Result
    },
};

pub struct Engine {
    /// Occupied spaces DB
    osdb: Connection,
    compute_pool: ThreadPool,
    time_direction: TimeDirection,
}

impl Engine {
    pub fn new() -> Result<Self> {
        let engine = Self {
            osdb: Connection::open_in_memory()
                .map_err(|err| make_error![Error::Physics::Init(err)])?,
            compute_pool: ThreadPool::new(num_cpus::get()),
            time_direction: TimeDirection::Forward,
        };

        Ok(engine)
    }

    pub fn update_time_direction(&mut self, time_step: &chrono::Duration) {
        lazy_static! {
            static ref ZERO: chrono::Duration = chrono::Duration::zero();
        }

        if *time_step > *ZERO {
            self.time_direction = TimeDirection::Forward;
        } else {
            self.time_direction = TimeDirection::Backward;
        }
    }

    pub fn is_uncomputed(&self, object: &Object4d, vtime: &chrono::Duration) -> bool {
        let track = object.track();
        if !track.computed_range().contains(&vtime) {
            return true;
        }

        let track_time_start = track.time_start();
        let computed_duration = track.time_end() - track_time_start;
        let half_computed_duration = computed_duration / 2;
        let uncomputed_border = track_time_start + half_computed_duration;
        let offset = track_time_start + *vtime;

        match self.time_direction {
            TimeDirection::Forward => offset > uncomputed_border,
            TimeDirection::Backward => offset < uncomputed_border,
        }
    }

    pub fn compute(
        &mut self, 
        mut objects: Vec<Arc<Mutex<Object4d>>>,
        attractors: Vec<Arc<Attractor>>,
    ) {
        // TODO compute collisions after each step

        let add_node: fn(&mut Track, TrackNode); 
        let last_node: fn(&Track) -> Shared<TrackNode>; 
        let last_atom: for<'n> fn(&'n TrackNode) -> &'n TrackAtom;
        
        match self.time_direction {
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
                        let step = track.relative_compute_step();

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

pub enum TimeDirection {
    Forward,
    Backward,
}
