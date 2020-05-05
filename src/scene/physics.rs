use {
    std::{
        collections::hash_map::HashMap,
        sync::mpsc,
    },
    threadpool::ThreadPool,
    log::{
        trace,
        info,
        error,
    },
    crate::{
        shared_access,
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
            SessionId,
            SessionName,
            ObjectId,
            ObjectName,
            AttractorId,
            AttractorName,
            Mass,
            TimeFormat,
            RelativeTime,
            AsRelativeTime,
        },
        storage::{
            StorageManager,
            OccupiedSpacesStorage,
            OccupiedSpace,
        },
        shared::Shared,
        Result
    },
};

const LOG_TARGET: &'static str = "engine";

const STORAGE_CONNECTION_STRING: &'static str = "host=localhost user=postgres";

pub struct Engine {
    objects: HashMap<ObjectId, Shared<Object4d>>,
    attractors: HashMap<AttractorId, Shared<Attractor>>,
    master_storage: StorageManager,
    oss: OccupiedSpacesStorage,
    session_id: SessionId,
    compute_pool: ThreadPool,
    update_time_ratio: f32,
}

impl Engine {
    pub fn new() -> Result<Self> {
        let mut master_storage = StorageManager::setup(STORAGE_CONNECTION_STRING)?;
        let default_session_name = None;
        let session_id = master_storage
            .session()
            .new(default_session_name)?;

        let oss = OccupiedSpacesStorage::new()?;

        let engine = Self {
            objects: HashMap::new(),
            attractors: HashMap::new(),
            master_storage,
            oss,
            session_id,
            compute_pool: ThreadPool::new(num_cpus::get()),
            update_time_ratio: 0.25,
        };

        Ok(engine)
    }



    pub fn lower_update_time_threshold(&self) -> f32 {
        self.update_time_ratio
    }

    pub fn upper_update_time_threshold(&self) -> f32 {
        1.0 - self.update_time_ratio
    }

    pub fn track_relative_time(&self, object: &Object4d, vtime: &chrono::Duration) -> RelativeTime {
        assert!((0.0..1.0).contains(&self.update_time_ratio));

        let track = object.track();
        let track_time_start = track.time_start();
        let computed_duration = (track.time_end() - track_time_start).num_milliseconds() as RelativeTime;
        let offset = (*vtime - track_time_start).num_milliseconds() as RelativeTime;
        let relative_time = offset / computed_duration;

        relative_time
    }

    pub fn add_attractor(
        &mut self, 
        attractor: Attractor,
        attractor_name: AttractorName
    ) -> Result<Shared<Attractor>> {
        let id = self.master_storage.attractor().add(
            self.session_id, 
            &attractor, 
            &attractor_name
        )?;

        let attractor = Shared::from(attractor);
        self.attractors.insert(id, attractor.share());
        
        Ok(attractor)
    }

    pub fn add_object(
        &mut self,
        mut object: Object4d,
        step: chrono::Duration,
        initial_location: Vector,
    ) -> Result<(ObjectId, Shared<Object4d>)> {
        // TODO assert there is no collisions

        let mut atom = TrackAtom::with_location(initial_location);

        let attractors = self.attractors_refs_copy();
        Self::atom_set_velocity(object.mass(), &mut atom, step.as_relative_time(), attractors)?;
        object.track_mut().push_back(atom.into());

        let id = self.master_storage.object().add(
            self.session_id, 
            &object,
        )?;

        let object = Shared::from(object);
        self.objects.insert(id, object.share());

        Ok((id, object))
    }

    pub fn rename_object_in_master_storage(&mut self, object_id: ObjectId, new_name: &str) -> Result<()> {
        self.master_storage.object().rename(self.session_id, object_id, new_name)
    }

    pub fn print_object_list(&mut self) -> Result<()> {
        self.master_storage.object().print_list(self.session_id)
    }

    pub fn new_session(&mut self, session_name: Option<SessionName>) -> Result<()> {
        let new_session_id = self.master_storage.session().new(session_name)?;
        self.master_storage.session().unlock(self.session_id)?;
        self.session_id = new_session_id;

        Ok(())
    }

    pub fn print_current_session_name(&mut self) -> Result<()> {
        self.master_storage.session().print_current_name(self.session_id)
    }

    pub fn print_session_list(&mut self) -> Result<()> {
        self.master_storage.session().print_list()
    }

    pub fn save_current_session(&mut self, session_name: ObjectName) -> Result<()> {
        self.master_storage.session().save(self.session_id, session_name.as_str())
    }

    pub fn load_session(&mut self, session_name: ObjectName) -> Result<()> {
        let new_session_id = self.master_storage.session().load(session_name.as_str())?;
        self.master_storage.session().unlock(self.session_id)?;
        self.session_id = new_session_id;

        Ok(())
    }

    pub fn rename_session(&mut self, old_name: &str, new_name: &str) -> Result<()> {
        self.master_storage
            .session()
            .rename(old_name, new_name)
    }

    pub fn delete_session(&mut self, session_name: &str) -> Result<()> {
        self.master_storage.session().delete(session_name)
    }

    pub fn shutdown(&mut self) -> Result<()> {
        self.master_storage.session().unlock(self.session_id)
    }

    pub fn update_session_access_time(&mut self) -> Result<()> {
        trace! {
            target: LOG_TARGET,
            "update session access time"
        };

        self.master_storage
            .session()
            .update_access_time(self.session_id)
    }

    pub fn update_objects(&mut self, vtime: &chrono::Duration) -> Result<()> {
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

        for (id, object) in self.objects.iter() {
            let rt = self.track_relative_time(
                &*shared_access![object], 
                vtime
            );

            if rt.is_nan() || rt < self.lower_update_time_threshold() || rt > self.upper_update_time_threshold() {
                let mut sync_object = shared_access![mut object];
                let track = sync_object.track_mut();
                let half_computed_time = track.time_start() + (track.time_end() - track.time_start()) / 2;

                if rt.is_nan() || rt > self.upper_update_time_threshold() {
                    compute_direction = TimeDirection::Forward;
                    track.truncate(..half_computed_time);

                    log_update!(id, half_computed_time => future);
                } else {
                    compute_direction = TimeDirection::Backward;
                    track.truncate(half_computed_time..);

                    log_update!(id, half_computed_time => past);
                }

                sync_object.set_computing();
                std::mem::drop(sync_object);
                uncomputed_objects.push((*id, object.share()));
            }
        }

        let attractors = self.attractors_refs_copy();
        self.compute(
            compute_direction, 
            uncomputed_objects, 
            attractors
        );

        Ok(())
    }

    fn compute(
        &mut self, 
        compute_time_direction: TimeDirection,
        mut objects: Vec<(ObjectId, Shared<Object4d>)>,
        attractors: Vec<Shared<Attractor>>,
    ) {
        if objects.is_empty() {
            return;
        }

        let add_node: fn(&mut Track, TrackNode); 
        let last_node: fn(&Track) -> Shared<TrackNode>; 
        let last_atom: for<'n> fn(&'n TrackNode) -> &'n TrackAtom;
        let last_time: fn(&Track) -> RelativeTime;
        let new_time: fn(&Track, RelativeTime) -> RelativeTime;
        
        match compute_time_direction {
            TimeDirection::Forward => {
                add_node = Track::push_back;
                last_node = Track::node_end;
                last_atom = TrackNode::atom_end;
                last_time = |track| track.time_end().as_relative_time();
                new_time = |track, time| {
                    let step = track.relative_compute_step();
                    time + step
                };
            },
            TimeDirection::Backward => {
                add_node = Track::push_front;
                last_node = Track::node_start;
                last_atom = TrackNode::atom_start;
                last_time = |track| track.time_start().as_relative_time();
                new_time = |track, time| {
                    let step = track.relative_compute_step();
                    time - step
                };
            },
        }

        let mut temp_vec = vec![];

        let mut objects = &mut objects;
        let mut remaining = &mut temp_vec;

        while !objects.is_empty() {
            let (track_parts_sender, track_parts_receiver) = mpsc::channel();

            while let Some((obj_id, object)) = objects.pop() {
                if shared_access![object].track().is_fully_computed() {
                    shared_access![mut object].reset_computing();
                } else {
                    remaining.push((obj_id, object.share()));

                    let object = object.share();
                    let attractors = attractors.clone();
                    let track_parts_sender = track_parts_sender.clone();
                    self.compute_pool.execute(move || {
                        let mut sync_object = shared_access![mut object];
                        let obj_mass = sync_object.mass();

                        let track = sync_object.track_mut();

                        let node = last_node(track);
                        let sync_node = shared_access![node];

                        let time = last_time(track);
                        let new_time = new_time(track, time);

                        let atom = last_atom(&sync_node);

                        let step = match compute_time_direction { 
                            TimeDirection::Forward => track.relative_compute_step(),
                            TimeDirection::Backward => -track.relative_compute_step(),
                        };

                        let mut new_atom = atom.at_next_location(step);

                        match Self::atom_set_velocity(
                            obj_mass, 
                            &mut new_atom, 
                            step, 
                            attractors
                        ) {
                            Ok(()) => {
                                let track_part = TrackPart::new(
                                    obj_id,
                                    object.share(),
                                    node.share(),
                                    add_node,
                                    atom.clone(), 
                                    time,
                                    new_atom,
                                    new_time
                                );
                                
                                if let Err(err) = track_parts_sender.send(track_part) {
                                    error! {
                                        target: LOG_TARGET,
                                        "unable to send track part: {}", err
                                    }
                                }
                            },
                            Err(err) => error! {
                                target: LOG_TARGET,
                                "unable to add new track node to the object `{}`: {}", 
                                shared_access![object].name(),
                                err
                            }
                        }
                    });
                }
            }

            // Sender must only be available in thr computational threads.
            // If not, the `process_track_parts` will hang.
            std::mem::drop(track_parts_sender);

            std::mem::swap(&mut objects, &mut remaining);

            self.process_track_parts(track_parts_receiver);
        }
    }

    fn process_track_parts(&mut self, parts_channel: mpsc::Receiver<TrackPart>) {
        while let Ok(track_part) = parts_channel.recv() {
            let occupied_space = match track_part.get_occupied_space() {
                Ok(os) => os,
                Err(err) => {
                    error! {
                        target: LOG_TARGET,
                        "unable to get occupied space from the new track part (OID#{}): {}",
                        track_part.object_id,
                        err
                    };

                    continue;
                }
            };

            trace! {
                target: LOG_TARGET,
                "new occupied space (OID#{}): {}",
                track_part.object_id,
                occupied_space
            }

            let possible_collisions = match self.oss.check_possible_collisions(&occupied_space) {
                Ok(possible_collisions) => possible_collisions,
                Err(err) => {
                    error! {
                        target: LOG_TARGET,
                        "unable to check possible collisions for a new occupied space (OID#{}): {}",
                        track_part.object_id,
                        err
                    }

                    continue;
                }
            };

            // TODO resolve for collision
            for possible_collision in possible_collisions.iter() {
                info! {
                    target: LOG_TARGET,
                    "possible collision detected [OID#{} w/ OID#{}], t ∈ [{}, {})",
                    track_part.object_id,
                    possible_collision.object_id,
                    possible_collision.t_min,
                    possible_collision.t_max,
                }
            }

            if let Err(err) = self.oss.add_occupied_space(occupied_space) {
                error! {
                    target: LOG_TARGET,
                    "unable to add new occupied space to OccupiedSpacesStorage (OID#{}): {}",
                    track_part.object_id,
                    err
                };

                continue;
            }

            let object_id = track_part.object_id;
            if let Err(err) = track_part.add_new_node() {
                error! {
                    target: LOG_TARGET,
                    "unable to add new track node to the object (id: {}): {}", 
                    object_id, 
                    err
                };
            }
        }
    }

    fn atom_set_velocity(obj_mass: Mass, atom: &mut TrackAtom, step: RelativeTime, attractors: Vec<Shared<Attractor>>) -> Result<()> {
        let mut acceleration = compute_acceleration(obj_mass, atom.location(), &attractors)?;
        acceleration.scale_mut(step);
        atom.set_velocity(atom.velocity() + acceleration);

        Ok(())
    }

    fn attractors_refs_copy(&self) -> Vec<Shared<Attractor>> {
        self.attractors.values()
            .map(|attr| attr.share())
            .collect()
    }
}

fn compute_acceleration(obj_mass: Mass, location: &Vector, attractors: &Vec<Shared<Attractor>>) -> Result<Vector> {
    let mut acceleration = Vector::zeros();
    for attractor in attractors.iter() {
        let attractor = shared_access![attractor];

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

    Ok(acceleration)
}

#[derive(Debug, Clone, Copy)]
pub enum TimeDirection {
    Forward,
    Backward,
}

struct TrackPart {
    object_id: ObjectId,
    object: Shared<Object4d>,
    last_node: Shared<TrackNode>,
    add_node: fn(&mut Track, TrackNode),
    old_atom: TrackAtom,
    old_time: RelativeTime,
    new_atom: TrackAtom,
    new_time: RelativeTime,
}

impl TrackPart {
    pub fn new(
        object_id: ObjectId,
        object: Shared<Object4d>,
        last_node: Shared<TrackNode>,
        add_node: fn(&mut Track, TrackNode),
        old_atom: TrackAtom, 
        old_time: RelativeTime,
        new_atom: TrackAtom,
        new_time: RelativeTime,
    ) -> Self {
        Self {
            object_id,
            object, 
            last_node,
            add_node,
            old_atom, 
            old_time,
            new_atom,
            new_time,
        }
    }

    pub fn add_new_node(self) -> Result<()> {
        let mut object = shared_access![mut self.object];
        let track = object.track_mut();

        (self.add_node)(track, self.new_atom.into());

        Ok(())
    }

    pub fn get_occupied_space(&self) -> Result<OccupiedSpace> {
        let object_radius = shared_access![self.object].radius();

        let os = OccupiedSpace::with_track_part(
            self.object_id, 
            object_radius, 
            self.old_atom.location(), 
            self.old_time, 
            *self.old_atom.velocity(),
            self.new_atom.location(), 
            self.new_time,
            *self.new_atom.velocity(),
        );

        Ok(os)
    }
}