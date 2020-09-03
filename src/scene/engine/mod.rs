use {
    std::{
        collections::hash_map::HashMap,
        sync::mpsc,
    },
    threadpool::ThreadPool,
    log::{
        trace,
        info,
        warn,
        error,
    },
    crate::{
        shared_access,
        make_error,
        scene::{
            Object4d,
            Attractor,
            track::{
                TrackNode,
                TrackAtom,
                Collision,
                CanceledCollisions,
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
            Distance,
            Mass,
            TimeFormat,
            RelativeTime,
            AsRelativeTime,
            AsAbsoluteTime,
            TimeDirection,
        },
        storage::{
            StorageManager,
            OccupiedSpacesStorage,
            OccupiedSpace,
        },
        math::{
            ranged_secant,
            hermite_interpolation,
        },
        shared::Shared,
        Result
    },
};

mod uncomputed;
mod task;
mod collision;

use uncomputed::*;
use task::Task;
use collision::CollisionDescriptor;

const LOG_TARGET: &'static str = "engine";

const STORAGE_CONNECTION_STRING: &'static str = "host=localhost user=postgres";

pub type Objects = Shared<HashMap<ObjectId, Shared<Object4d>>>;
pub type Attractors = Shared<HashMap<AttractorId, Shared<Attractor>>>;

pub struct Engine {
    objects: Objects,
    attractors: Attractors,
    master_storage: StorageManager,
    oss: OccupiedSpacesStorage,
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
            oss: oss.clone(),
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

        object.set_id(id);

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
                    self.oss.delete_past_occupied_space(*id, border_time.as_relative_time())?;

                    log_update!(id, border_time => future);
                } else {
                    compute_direction = TimeDirection::Backward;

                    // An addition of `track.compute_step()` is necessary, 
                    // because of track internal structure.
                    //
                    // Without the addition, the `rt` variable will be greater than 0.75,
                    // that will initiate computing to /future/, what is not desirable
                    let border_time = border_time + track.compute_step();

                    track.truncate(border_time..);
                    self.oss.delete_future_occupied_space(*id, border_time.as_relative_time())?;

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
            let objects = self.objects.share();

            move || {
                Self::computational_thread(thread_pool, oss, task_receiver, objects, attractors)
            }
        });
    }

    fn computational_thread(
        thread_pool: ThreadPool, 
        oss: OccupiedSpacesStorage,
        task_receiver: mpsc::Receiver<Task>,
        objects: Objects,
        attractors: Attractors, 
    ) {
        let (forward_task_sender, forward_task_receiver) = mpsc::channel();
        let (backward_task_sender, backward_task_receiver) = mpsc::channel();

        std::thread::spawn({
            let thread_pool = thread_pool.clone();
            let oss = oss.clone();
            let attractors = attractors.share();
            let objects = objects.share();

            move || {
                Self::process_uncomputed::<ForwardUncomputedTrack>(
                    thread_pool, 
                    oss,
                    forward_task_receiver, 
                    objects,
                    attractors
                )
            }
        });

        std::thread::spawn(move || {
            Self::process_uncomputed::<BackwardUncomputedTrack>(
                thread_pool, 
                oss,
                backward_task_receiver, 
                objects,
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
        objects: Objects,
        attractors: Attractors
    )
    where
        U: UncomputedTrack
    {
        let mut uncomputed_objects = vec![];
        let mut remaining = vec![];

        loop {
            let (track_parts_sender, track_parts_receiver) = mpsc::channel();

            if uncomputed_objects.is_empty() {
                let task = match task_receiver.recv() {
                    Ok(task) => task,
                    Err(_) => return
                };

                uncomputed_objects = task.objects;
            }

            while let Ok(mut task) = task_receiver.try_recv() {
                uncomputed_objects.append(&mut task.objects);
            }

            while let Some((obj_id, object)) = uncomputed_objects.pop() {
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
                        let obj_radius = sync_object.radius();

                        let track = sync_object.track_mut();

                        let node = <U as UncomputedTrack>::last_node(track);

                        let time = <U as UncomputedTrack>::last_time(track);
                        let new_time = <U as UncomputedTrack>::new_time(track);

                        let step = <U as UncomputedTrack>::time_step(track);

                        let mut new_atom = node.at_next_location(step);

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
                                    obj_radius,
                                    node.clone(), 
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

            std::mem::swap(&mut uncomputed_objects, &mut remaining);

            Self::process_track_parts::<U>(
                &oss, 
                track_parts_receiver,
                &objects,
                &attractors,
            );
        }
    }

    fn process_track_parts<U>(
        oss: &OccupiedSpacesStorage, 
        track_parts_receiver: mpsc::Receiver<TrackPart>,
        objects: &Objects, 
        attractors: &Attractors,
    ) 
    where
        U: UncomputedTrack
    {
        while let Ok(mut track_part) = track_parts_receiver.recv() {
            let mut occupied_space = track_part.get_occupied_space();

            trace! {
                target: LOG_TARGET,
                "new occupied space (OID#{}): {}",
                track_part.object_id,
                occupied_space
            }

            let mut possible_collisions = match oss.check_possible_collisions(&occupied_space) {
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

            use crate::shared_access;

            lazy_static::lazy_static! {
                static ref FIRST_TIME: Shared<bool> = true.into();
            }

            if !*shared_access![FIRST_TIME] {
                possible_collisions.clear();
            }

            if let Some(collision_descriptor) = Self::clarify_earlest_collision::<U>(
                &occupied_space, 
                possible_collisions
            ) {
                *shared_access![mut FIRST_TIME] = false;
                
                info! {
                    target: LOG_TARGET,
                    "collision detected: [OID#{} w/ OID#{}], t = {}",
                    occupied_space.object_id,
                    collision_descriptor.colliding_object_id,
                    TimeFormat::VirtualTimeShort(collision_descriptor.collision_time.as_absolute_time())
                }

                match shared_access![objects].get(&collision_descriptor.colliding_object_id) {
                    Some(colliding_obj) => {
                        let colliding_object_id = collision_descriptor.colliding_object_id;
                        let collision_time = collision_descriptor.collision_time.as_absolute_time();

                        if let Err(err) = Self::resolve_collision::<U>(
                            oss,
                            &mut track_part,
                            colliding_obj,
                            collision_descriptor,
                            attractors,
                        ) {
                            error! {
                                target: LOG_TARGET,
                                "unable to resolve collision [OID#{} w/ OID#{}], t = {}: {}",
                                occupied_space.object_id,
                                colliding_object_id,
                                TimeFormat::VirtualTimeShort(collision_time),
                                err
                            }
                        }

                        occupied_space = track_part.get_occupied_space();
                    }
                    None => warn! {
                        target: LOG_TARGET,
                        "colliding object (OID#{}) is lost", collision_descriptor.colliding_object_id
                    }
                }
            }

            let object_id = track_part.object_id;
            if let Err(err) = track_part.add_new_node::<U>(&oss, occupied_space) {
                warn! {
                    target: LOG_TARGET,
                    "unable to add new track node to the object (OID#{}): {}", 
                    object_id, 
                    err
                };
            }
        }
    }

    fn atom_set_velocity(obj_mass: Mass, atom: &mut TrackAtom, step: RelativeTime, attractors: Attractors) -> Result<()> {
        let mut acceleration = Self::compute_acceleration(obj_mass, atom.location(), &attractors)?;
        acceleration.scale_mut(step);
        atom.set_velocity(atom.velocity() + acceleration);

        Ok(())
    }

    fn compute_acceleration(obj_mass: Mass, location: &Vector, attractors: &Attractors) -> Result<Vector> {
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

    // Returns colliding object id and the collision time
    fn clarify_earlest_collision<U>(obj_occupied_space: &OccupiedSpace, possible_collisions: Vec<OccupiedSpace>) -> Option<CollisionDescriptor> 
    where
        U: UncomputedTrack
    {
        const EPS: f32 = 0.0001;
        let obj_t_min = obj_occupied_space.t_min;
        let obj_t_max = obj_occupied_space.t_max; 
        let (obj_loc_begin, obj_loc_end) = obj_occupied_space.restore_locations();
        let obj_vel_begin = obj_occupied_space.begin_velocity;
        let obj_vel_end = obj_occupied_space.end_velocity;
        let obj_radius = obj_occupied_space.cube_size;

        let earlest_predicate: fn(RelativeTime, RelativeTime) -> bool;
        match <U as UncomputedTrack>::time_direction() {
            TimeDirection::Forward => {
                earlest_predicate = |new_time, old_time| new_time < old_time;
            },
            TimeDirection::Backward => {
                earlest_predicate = |new_time, old_time| new_time > old_time;
            }
        }

        let mut collision: Option<CollisionDescriptor> = None;

        for possible_collision in possible_collisions.iter() {
            info! {
                target: LOG_TARGET,
                "possible collision detected [OID#{} w/ OID#{}], t ∈ [{}, {})",
                obj_occupied_space.object_id,
                possible_collision.object_id,
                TimeFormat::VirtualTimeShort(possible_collision.t_min.as_absolute_time()),
                TimeFormat::VirtualTimeShort(possible_collision.t_max.as_absolute_time()),
            }

            let colliding_t_min = possible_collision.t_min;
            let colliding_t_max = possible_collision.t_max;

            let (colliding_loc_begin, colliding_loc_end) = possible_collision.restore_locations();
            let colliding_vel_begin = possible_collision.begin_velocity;
            let colliding_vel_end = possible_collision.end_velocity;
            let colliding_radius = possible_collision.cube_size;

            let collision_distance = obj_radius + colliding_radius;
            let t_min = obj_t_min.max(colliding_t_min);
            let t_max = obj_t_max.min(colliding_t_max);

            if let Some((collision_time, object_location, colliding_object_location)) = ranged_secant(
                t_min..t_max, 
                EPS, 
                |t| {
                    let obj_location = hermite_interpolation(
                        &obj_loc_begin, 
                        &obj_vel_begin, 
                        obj_t_min, 
                        &obj_loc_end, 
                        &obj_vel_end, 
                        obj_t_max, 
                        t
                    );

                    let colliding_location = hermite_interpolation(
                        &colliding_loc_begin, 
                        &colliding_vel_begin, 
                        colliding_t_min, 
                        &colliding_loc_end, 
                        &colliding_vel_end, 
                        colliding_t_max, 
                        t
                    );

                    let distance = (colliding_location - obj_location).norm();

                    (distance - collision_distance, obj_location, colliding_location)
                }
            ) {
                // println!("OBJECT_ID: {}", obj_occupied_space.object_id);
                // println!("LOC BEG: {}", obj_loc_begin);
                // println!("LOC END: {}", obj_loc_end);

                // println!("-------------------------------------------");

                // println!("OBJECT_ID: {}", possible_collision.object_id);
                // println!("LOC BEG: {}", colliding_loc_begin);
                // println!("LOC END: {}", colliding_loc_end);

                match collision {
                    Some(descriptor) if earlest_predicate(collision_time, descriptor.collision_time) => {
                        collision = Some(CollisionDescriptor {
                            object_location,
                            colliding_object_location,
                            colliding_object_id: possible_collision.object_id,
                            collision_time,
                        });
                    },
                    None => collision = Some(CollisionDescriptor {
                        object_location,
                        colliding_object_location,
                        colliding_object_id: possible_collision.object_id,
                        collision_time,
                    }),
                    _ => {}
                }
            }
        }

        collision
    }

    fn resolve_collision<U>(
        oss: &OccupiedSpacesStorage,
        track_part: &mut TrackPart, 
        colliding_obj: &Shared<Object4d>, 
        collision_descriptor: CollisionDescriptor,
        attractors: &Attractors,
    ) -> Result<()>
    where
        U: UncomputedTrack
    {
        let collision_time = collision_descriptor.collision_time.as_absolute_time();
        let object_mass = shared_access![track_part.object].mass();
        let object_location = collision_descriptor.object_location;

        let colliding_object_mass = shared_access![colliding_obj].mass();
        let colliding_object_location = collision_descriptor.colliding_object_location;

        let object_velocity = Self::compute_acceleration(
            object_mass, 
            &object_location, 
            attractors
        )?;

        let colliding_object_velocity = Self::compute_acceleration(
            colliding_object_mass, 
            &colliding_object_location, 
            attractors
        )?;

        let collision_direction = (colliding_object_location - object_location).normalize();
        
        let (
            mut object_normal_velocity, 
            object_tangent_velocity
        ) = Self::collision_velocity_components(
            collision_direction, 
            object_velocity
        );

        let (
            mut colliding_object_normal_velocity, 
            colliding_object_tangent_velocity
        ) = Self::collision_velocity_components(
            collision_direction, 
            colliding_object_velocity
        );

        let obj_normal_velocity_len = collision_direction.dot(&object_normal_velocity);
        let col_normal_velocity_len = collision_direction.dot(&colliding_object_normal_velocity);
        let len_diff = obj_normal_velocity_len - col_normal_velocity_len;
        let mass_sum = object_mass + colliding_object_mass;

        const VELOCITY_SCALE: f32 = 100.0;

        object_normal_velocity = collision_direction.scale(
            obj_normal_velocity_len - 2.0 * len_diff * colliding_object_mass / mass_sum 
        );

        colliding_object_normal_velocity = collision_direction.scale(
            col_normal_velocity_len + 2.0 * len_diff * object_mass / mass_sum
        );

        let object_velocity = object_tangent_velocity + object_normal_velocity.scale(VELOCITY_SCALE);
        let colliding_object_velocity = colliding_object_tangent_velocity + colliding_object_normal_velocity.scale(VELOCITY_SCALE);

        // let object_velocity = object_velocity - collision_direction.scale(
        //     VELOCITY_SCALE * collision_direction.dot(&object_velocity)
        // );
        // let colliding_object_velocity = colliding_object_velocity - collision_direction.scale(
        //     VELOCITY_SCALE * collision_direction.dot(&colliding_object_velocity)
        // );

        let src_atom = match track_part.new_node {
            TrackNode::Atom(ref atom) => atom.clone(),
            _ => unreachable!()
        };

        let prev_node_src_step = <U as UncomputedTrack>::time_step(
            shared_access![track_part.object].track()
        ).as_absolute_time();

        // println!("------------------------------------------");
        // println!("OBJECT_ID: {}", track_part.object_id);
        // println!("SOURCE {}", src_atom.location());
        // println!("COLLISION {}", object_location);

        // println!("------------------------------------------");
        // println!("OBJECT_ID: {}", shared_access![colliding_obj].id());

        track_part.new_node = Collision::new(
            colliding_obj.share_weak(), 
            <U as UncomputedTrack>::time_direction(), 
            collision_time, 
            src_atom, 
            prev_node_src_step,
            TrackAtom::new(object_location, object_velocity),
        ).into();

        let canceled_collisions = shared_access![mut colliding_obj]
            .track_mut()
            .place_collision(
                track_part.object.share_weak(),
                collision_time,
                <U as UncomputedTrack>::time_direction(),
                TrackAtom::new(colliding_object_location, colliding_object_velocity)  
            );

        // TODO create new OS for colliding_object???

        // Self::delete_canceled_occupied_spaces::<U>(oss, track_part.object_id, collision_time)?;
        Self::delete_canceled_occupied_spaces::<U>(oss, collision_descriptor.colliding_object_id, collision_time)?;
        Self::cancel_dependent_collisions::<U>(oss, canceled_collisions)?;
            
        Ok(())
    }

    fn collision_velocity_components(
        collision_direction: Vector,
        velocty: Vector
    ) -> (Vector, Vector) {
        let normal = collision_direction.scale(velocty.dot(&collision_direction));
        let tangent = velocty - normal;

        (normal, tangent)
    }

    fn cancel_dependent_collisions<'rb, U>(
        oss: &OccupiedSpacesStorage,
        mut canceled_collisions: CanceledCollisions,
    ) -> Result<()> 
    where
        U: UncomputedTrack
    {
        let mut remaining = vec![];

        while !canceled_collisions.is_empty() {
            while let Some(collision) = canceled_collisions.pop() {
                if let Some(object) = collision.colliding_object.upgrade() {
                    let mut object = shared_access![mut object];
                    let track = object.track_mut();

                    let new_truncated = match collision.time_direction {
                        TimeDirection::Forward => track.truncate(collision.when..),
                        TimeDirection::Backward => track.truncate(..collision.when),
                    };

                    let mut canceled_collisions = new_truncated.filter_map(|node| match node {
                        TrackNode::Collision(collision) => Some(collision.clone()),
                        _ => None
                    }).collect();

                    remaining.append(&mut canceled_collisions);

                    Self::delete_canceled_occupied_spaces::<U>(oss, object.id(), collision.when)?;
                }
            }

            std::mem::swap(&mut canceled_collisions, &mut remaining);
        }

        Ok(())
    }

    fn delete_canceled_occupied_spaces<U>(
        oss: &OccupiedSpacesStorage,
        object_id: ObjectId,
        from_when: chrono::Duration 
    ) -> Result<()>
    where 
        U: UncomputedTrack 
    {
        let t = from_when.as_relative_time();

        match <U as UncomputedTrack>::time_direction() {
            TimeDirection::Forward => oss.delete_future_occupied_space(object_id, t),
            TimeDirection::Backward => oss.delete_past_occupied_space(object_id, t),
        }
    }
}

struct TrackPart {
    object_id: ObjectId,
    object: Shared<Object4d>,
    object_radius: Distance,
    old_node: TrackNode,
    old_time: RelativeTime,
    new_node: TrackNode,
    new_time: RelativeTime,
}

impl TrackPart {
    pub fn new(
        object_id: ObjectId,
        object: Shared<Object4d>,
        object_radius: Distance,
        old_node: TrackNode, 
        old_time: RelativeTime,
        new_atom: TrackAtom,
        new_time: RelativeTime,
    ) -> Self {
        Self {
            object_id,
            object, 
            object_radius,
            old_node, 
            old_time,
            new_node: new_atom.into(),
            new_time,
        }
    }

    pub fn add_new_node<U>(
        self, 
        oss: &OccupiedSpacesStorage, 
        occupied_space: OccupiedSpace
    ) -> Result<()> 
    where
        U: UncomputedTrack
    {
        let mut object = shared_access![mut self.object];
        let track = object.track_mut();

        if U::last_time(track) == self.old_time {
            oss.add_occupied_space(occupied_space)?;

            <U as UncomputedTrack>::add_node(track, self.new_node);

            Ok(())
        } else {
            Err(make_error![
                Error::Physics::TrackPartIsNotAligned(
                    object.name().clone(), 
                    self.old_time..self.new_time
                )
            ])
        }

    }

    pub fn get_occupied_space(&self) -> OccupiedSpace {
        let new_atom = match self.new_node {
            TrackNode::Atom(ref atom) => atom,
            TrackNode::Collision(ref collision) => &collision.track_atom,
        };

        let os = OccupiedSpace::with_track_part(
            self.object_id, 
            self.object_radius, 
            self.old_node.location(), 
            self.old_time, 
            *self.old_node.velocity(),
            new_atom.location(), 
            self.new_time,
            *new_atom.velocity(),
        );

        os
    }
}