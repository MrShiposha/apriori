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

mod uncomputed;
mod task;

use uncomputed::*;
use task::Task;

const LOG_TARGET: &'static str = "engine";

const STORAGE_CONNECTION_STRING: &'static str = "host=localhost user=postgres";

pub type Objects = Shared<HashMap<ObjectId, Shared<Object4d>>>;
pub type Attractors = Shared<HashMap<AttractorId, Shared<Attractor>>>;

pub struct Engine {
    objects: Objects,
    attractors: Attractors,
    master_storage: StorageManager,
    session_id: SessionId,
    thread_pool: ThreadPool,
    task_sender: mpsc::Sender<Task>,
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

        let (task_sender, task_receiver) = mpsc::channel();

        let engine = Self {
            objects: Shared::new(),
            attractors: Shared::new(),
            master_storage,
            session_id,
            thread_pool: ThreadPool::default(),
            task_sender,
            update_time_ratio: 0.25,
        };

        engine.spawn_computational_thread(oss, task_receiver);

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
        let time_length = track.time_length().num_milliseconds() as RelativeTime;
        let time_offset = track.time_offset(vtime).num_milliseconds() as RelativeTime;
        let relative_time = time_offset / time_length;

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
        shared_access![mut self.attractors].insert(id, attractor.share());
        Ok(attractor)
    }

    pub fn add_object(
        &mut self,
        mut object: Object4d,
        time: chrono::Duration,
        step: chrono::Duration,
        initial_location: Vector,
    ) -> Result<(ObjectId, Shared<Object4d>)> {
        // TODO assert there is no collisions

        let mut atom = TrackAtom::with_location(initial_location);

        Self::atom_set_velocity(
            object.mass(), 
            &mut atom, 
            step.as_relative_time(), 
            self.attractors.share()
        )?;

        object.track_mut().set_initial_node(atom.into(), time);

        let id = self.master_storage.object().add(
            self.session_id, 
            &object,
        )?;

        let object = Shared::from(object);
        shared_access![mut self.objects].insert(id, object.share());
        
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
        // // TODO load from DB

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

        let objects = shared_access![self.objects];
        for (id, object) in objects.iter() {
            let rt = self.track_relative_time(
                &*shared_access![object], 
                vtime
            );

            if rt.is_nan() || rt < self.lower_update_time_threshold() || rt > self.upper_update_time_threshold() {
                if shared_access![object].is_computing() {
                    continue;
                }

                let mut sync_object = shared_access![mut object];
                let track = sync_object.track_mut();
                let border_time = track.time_start() + track.time_length() / 2;

                if rt.is_nan() || rt > self.upper_update_time_threshold() {
                    compute_direction = TimeDirection::Forward;
                    track.truncate(..border_time);

                    log_update!(id, border_time => future);
                } else {
                    compute_direction = TimeDirection::Backward;

                    // LOOK AT THE PAPER PICTURE
                    let border_time = border_time + *track.compute_step();
                    track.truncate(border_time..);

                    log_update!(id, border_time => past);
                }

                std::mem::drop(sync_object);
                uncomputed_objects.push((*id, object.share()));
            }
        }

        std::mem::drop(objects);

        self.task_sender.send(
            Task::new(compute_direction, uncomputed_objects)
        ).unwrap();

        Ok(())
    }

    fn spawn_computational_thread(&self, oss: OccupiedSpacesStorage, task_receiver: mpsc::Receiver<Task>) {
        std::thread::spawn({
            let thread_pool = self.thread_pool.clone();
            let attractors = self.attractors.share();

            move || {
                Self::computational_thread(thread_pool, oss, attractors, task_receiver)
            }
        });
    }

    fn computational_thread(
        thread_pool: ThreadPool, 
        oss: OccupiedSpacesStorage,
        attractors: Attractors, 
        task_receiver: mpsc::Receiver<Task>
    ) {
        let (forward_task_sender, forward_task_receiver) = mpsc::channel();
        let (backward_task_sender, backward_task_receiver) = mpsc::channel();

        std::thread::spawn({
            let thread_pool = thread_pool.clone();
            let oss = oss.clone();
            let attractors = attractors.share();

            move || {
                Self::process_uncomputed::<ForwardUncomputedTrack>(
                    thread_pool, 
                    oss,
                    forward_task_receiver, 
                    attractors
                )
            }
        });

        std::thread::spawn(move || {
            Self::process_uncomputed::<BackwardUncomputedTrack>(
                thread_pool, 
                oss,
                backward_task_receiver, 
                attractors
            )
        });

        loop {
            let task = match task_receiver.recv() {
                Ok(task) => task,
                Err(_) => return
            };

            match task.time_direction {
                TimeDirection::Forward => forward_task_sender.send(task).unwrap(),
                TimeDirection::Backward => backward_task_sender.send(task).unwrap(),
            }
        }
    }

    fn process_uncomputed<U>(
        thread_pool: ThreadPool, 
        oss: OccupiedSpacesStorage,
        task_receiver: mpsc::Receiver<Task>,
        attractors: Attractors
    )
    where
        U: UncomputedTrack
    {
        let mut objects = vec![];
        let mut remaining = vec![];

        loop {
            let (track_parts_sender, track_parts_receiver) = mpsc::channel();

            if objects.is_empty() {
                let task = match task_receiver.recv() {
                    Ok(task) => task,
                    Err(_) => return
                };

                objects = task.objects;
            }

            while let Ok(mut task) = task_receiver.try_recv() {
                objects.append(&mut task.objects);
            }

            while let Some((obj_id, object)) = objects.pop() {
                if shared_access![object].track().is_fully_computed() {
                    shared_access![mut object].reset_computing();
                } else {
                    if !shared_access![object].is_computing() {
                        shared_access![mut object].set_computing();
                    }

                    remaining.push((obj_id, object.share()));

                    let object = object.share();
                    let attractors = attractors.clone();
                    let track_parts_sender = track_parts_sender.clone();
                    thread_pool.execute(move || {
                        let mut sync_object = shared_access![mut object];
                        let obj_mass = sync_object.mass();

                        let track = sync_object.track_mut();

                        let node = <U as UncomputedTrack>::last_node(track);
                        let sync_node = shared_access![node];

                        let time = <U as UncomputedTrack>::last_time(track);
                        let new_time = <U as UncomputedTrack>::new_time(track);

                        let atom = <U as UncomputedTrack>::last_atom(&sync_node);

                        let step = <U as UncomputedTrack>::time_step(track);

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
                                    atom.clone(), 
                                    time,
                                    new_atom,
                                    new_time
                                );
                                
                                track_parts_sender.send(track_part).unwrap();
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

            Self::process_track_parts::<U>(&oss, track_parts_receiver);
        }
    }

    fn process_track_parts<U>(oss: &OccupiedSpacesStorage, track_parts_receiver: mpsc::Receiver<TrackPart>) 
    where
        U: UncomputedTrack
    {
        while let Ok(track_part) = track_parts_receiver.recv() {
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

            let possible_collisions = match oss.check_possible_collisions(&occupied_space) {
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

            // TODO resolve collisions
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

            if let Err(err) = oss.add_occupied_space(occupied_space) {
                error! {
                    target: LOG_TARGET,
                    "unable to add new occupied space to OccupiedSpacesStorage (OID#{}): {}",
                    track_part.object_id,
                    err
                };

                continue;
            }

            let object_id = track_part.object_id;
            if let Err(err) = track_part.add_new_node::<U>() {
                error! {
                    target: LOG_TARGET,
                    "unable to add new track node to the object (id: {}): {}", 
                    object_id, 
                    err
                };
            }
        }
    }

    fn atom_set_velocity(obj_mass: Mass, atom: &mut TrackAtom, step: RelativeTime, attractors: Attractors) -> Result<()> {
        let mut acceleration = Self::compute_acceleration(obj_mass, atom.location(), attractors)?;
        acceleration.scale_mut(step);
        atom.set_velocity(atom.velocity() + acceleration);

        Ok(())
    }

    fn compute_acceleration(obj_mass: Mass, location: &Vector, attractors: Attractors) -> Result<Vector> {
        let mut acceleration = Vector::zeros();

        let attractors = shared_access![attractors];
        for attractor in attractors.values() {
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
        old_atom: TrackAtom, 
        old_time: RelativeTime,
        new_atom: TrackAtom,
        new_time: RelativeTime,
    ) -> Self {
        Self {
            object_id,
            object, 
            last_node,
            old_atom, 
            old_time,
            new_atom,
            new_time,
        }
    }

    pub fn add_new_node<U>(self) -> Result<()> 
    where
        U: UncomputedTrack
    {
        let mut object = shared_access![mut self.object];
        let track = object.track_mut();

        <U as UncomputedTrack>::add_node(track, self.new_atom.into());

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