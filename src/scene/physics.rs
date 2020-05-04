use {
    std::{
        collections::hash_map::HashMap,
        sync::{Arc, RwLock},
    },
    rusqlite::{params, NO_PARAMS},
    threadpool::ThreadPool,
    log::{
        trace,
        info,
        error,
    },
    crate::{
        make_error, 
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
            self,
            StorageManager,
            OccupiedSpacesStorage,
            OccupiedSpace,
        },
        shared::Shared,
        Result
    },
};

const LOG_TARGET: &'static str = "physics";

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
        // TODO compute collisions after each step

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
            while let Some((obj_id, object)) = objects.pop() {
                let sync_object = shared_access![object];
                if !sync_object.track().is_fully_computed() {
                    remaining.push((obj_id, object.share()));

                    let object = object.share();
                    let attractors = attractors.clone();
                    self.compute_pool.execute(move || {
                        let mut sync_object = shared_access![mut object];
                        let obj_mass = sync_object.mass();
                        let obj_radius = sync_object.radius();

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

                        // std::mem::drop(track);

                        let mut new_atom = atom.at_next_location(step);

                        // self.place_track_part(
                        //     &mut *sync_object, 
                        //     node.share(), 
                        //     atom, 
                        //     new_atom
                        // );

                        // TODO: Check collisions
                        // let new_occupied_space = OccupiedSpace::with_track_part(
                        //     obj_id, 
                        //     obj_radius, 
                        //     atom.location(), 
                        //     time, 
                        //     new_atom.location(), 
                        //     new_time
                        // );

                        // self.oss.add_occupied_space(new_occupied_space)
                        //     .map_err(|err| make_error![Error::Storage::AddOccupiedSpace(err)])?;

                        match Self::atom_set_velocity(
                            obj_mass, 
                            &mut new_atom, 
                            step, 
                            attractors
                        ) {
                            Ok(()) => add_node(track, new_atom.into()),
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

            std::mem::swap(&mut objects, &mut remaining);
            self.compute_pool.join();
        }
    }

    fn atom_set_velocity(obj_mass: Mass, atom: &mut TrackAtom, step: RelativeTime, attractors: Vec<Shared<Attractor>>) -> Result<()> {
        let mut acceleration = compute_acceleration(obj_mass, atom.location(), &attractors)?;
        acceleration.scale_mut(step);
        atom.set_velocity(atom.velocity() + acceleration);

        Ok(())
    }

    // fn place_track_part(
    //     oss: Arc<OccupiedSpacesStorage>, 
    //     object: &mut Object4d,
    //     last_node: Shared<TrackNode>, 
    //     last_atom: &TrackAtom, 
    //     next_atom: TrackAtom
    // ) {

    // }

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

#[derive(Clone, Copy)]
pub enum TimeDirection {
    Forward,
    Backward,
}