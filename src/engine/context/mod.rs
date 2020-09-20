use {
    crate::{
        engine::{actor::Actor, math, phys::*},
        r#type::{
            Coord, IntoStorageDuration, LayerId, ObjectId, ObjectName,
            RelativeTime, AsRelativeTime, AsAbsoluteTime, SessionId, LocationId, Vector,
        },
        object::GenCoord,
        storage::StorageManager,
        Error, Result,
    },
    itertools::Itertools,
    log::info,
    lr_tree::*,
    std::{
        collections::{HashMap, HashSet},
        sync::{mpsc, Arc, RwLock},
    },
    rayon::prelude::*,
};

pub mod db_util;
pub mod time_range;

pub use time_range::*;

use db_util::*;

use crate::query;

pub type ActorsMap = HashMap<ObjectId, Actor>;
pub type ActorsNamesMap = HashMap<ObjectName, ObjectId>;
pub type TracksSpace = ObjSpace<Coord, TrackPartInfo>;
pub type TracksTree = LRTree<Coord, TrackPartInfo>;
pub type TrackPartId = NodeId;

pub const GLOBAL_TREE_DIM: usize = 4;
pub const TREE_MIN_RECS: usize = 2;
pub const TREE_MAX_RECS: usize = 5;

const LOG_TARGET: &'static str = "context";

#[derive(Debug, Clone)]
pub struct TrackPartInfo {
    pub object_id: ObjectId,
    pub start_location: Vector,
    pub end_location: Vector,
    pub start_velocity: Vector,
    pub end_velocity: Vector,
    pub final_velocity: Option<Vector>,
}

impl TrackPartInfo {
    pub fn new(object_id: ObjectId, lhs_coord: &GenCoord, rhs_coord: &GenCoord, final_velocity: Option<Vector>) -> Self {
        Self {
            object_id,
            start_location: lhs_coord.location().clone(),
            end_location: rhs_coord.location().clone(),
            start_velocity: lhs_coord.velocity().clone(),
            end_velocity: rhs_coord.velocity().clone(),
            final_velocity,
        }
    }
}

pub struct Context {
    session_id: SessionId,
    layer_id: LayerId,
    actors: ActorsMap,
    actors_names: ActorsNamesMap,
    tracks_tree: TracksTree,
    time_range: TimeRange,
    new_objects: Vec<ObjectId>,
}

impl Context {
    pub fn new(session_id: SessionId, layer_id: LayerId) -> Self {
        Self::with_time_range(session_id, layer_id, TimeRange::default())
    }

    pub fn with_time_range(
        session_id: SessionId,
        layer_id: LayerId,
        time_range: TimeRange,
    ) -> Self {
        Self {
            session_id,
            layer_id,
            actors: HashMap::new(),
            actors_names: HashMap::new(),
            tracks_tree: TracksTree::with_obj_space(Self::new_tracks_space()),
            time_range,
            new_objects: vec![],
        }
    }

    pub fn session_id(&self) -> SessionId {
        self.session_id
    }

    pub fn layer_id(&self) -> LayerId {
        self.layer_id
    }

    pub fn time_range(&self) -> &TimeRange {
        &self.time_range
    }

    pub fn actor(&self, id: &ObjectId) -> &Actor {
        self.actors.get(id).unwrap()
    }

    pub fn actors(&self) -> &ActorsMap {
        &self.actors
    }

    pub fn tracks_tree(&self) -> &TracksTree {
        &self.tracks_tree
    }

    pub fn take_new_object_id(&mut self) -> Option<ObjectId> {
        self.new_objects.pop()
    }

    pub fn location(
        mbr: &MBR<Coord>,
        track_part_info: &TrackPartInfo,
        t: RelativeTime,
    ) -> Vector {
        let location = math::hermite_interpolation(
            &track_part_info.start_location,
            &track_part_info.start_velocity,
            mbr.bounds(0).min,
            &track_part_info.end_location,
            &track_part_info.end_velocity,
            mbr.bounds(0).max,
            t,
        );

        location
    }

    /// Returns all ids of objects that were changed.
    pub fn cancel_tracks_except(&self, from: RelativeTime, except: HashSet<ObjectId>) -> HashSet<ObjectId> {
        let mut changed = HashSet::new();

        let to = self.tracks_tree().lock_obj_space().get_root_mbr().bounds(0).max;

        let mbr = mbr![t = [from; to]];

        self.tracks_tree.retain_mut(
            &mbr,
        |obj_space, id| {
                if obj_space.is_removed(&id) {
                    return false;
                }

                let mbr = obj_space.get_data_mbr(id);
                let track_part_info = obj_space.get_data_payload(id);

                let time_bounds = mbr.bounds(0);

                let max_time = time_bounds.max;
                let object_id = track_part_info.object_id;

                changed.insert(object_id);

                if except.contains(&object_id) {
                    max_time <= from
                } else {
                    if time_bounds.is_in_bound(&from) {
                        let actor = self.actor(&object_id);
                        let start_time = time_bounds.min.as_absolute_time();
                        let end_time = from.as_absolute_time();

                        let time_range = TimeRange::with_bounds(start_time, end_time);

                        let track_part_info = obj_space.get_data_payload_mut(id);

                        let last_gen_coord = GenCoord::new(
                            start_time,
                            track_part_info.start_location,
                            track_part_info.start_velocity
                        );

                        let next_gen_coord = next_gen_coord(&last_gen_coord, time_range.length());

                        track_part_info.end_location = next_gen_coord.location().clone();
                        track_part_info.end_velocity = next_gen_coord.velocity().clone();

                        let new_mbr = make_track_part_mbr(
                            &time_range,
                            actor.object().radius(),
                            track_part_info
                        );

                        obj_space.set_data_mbr(id, new_mbr);

                        actor.set_last_gen_coord(next_gen_coord);

                        true
                    } else {
                        false
                    }
                }
            }
        );

        changed
    }

    pub fn replicate(
        &self,
        new_session_id: SessionId,
        new_layer_id: LayerId,
        new_time_range: TimeRange,
    ) -> (Context, UpdateKind) {
        if new_session_id != self.session_id || new_layer_id != self.layer_id {
            info! {
                target: LOG_TARGET,
                "create a context on a new layer"
            }

            return (
                Self::with_time_range(new_session_id, new_layer_id, new_time_range.clone()),
                UpdateKind::Initial(new_time_range)
            );
        }

        let session_id = self.session_id;
        let layer_id = self.layer_id;
        let actors = self.actors.clone();
        let actors_names = self.actors_names.clone();
        let time_range = new_time_range;

        let new_context;
        // let split_ratio = self.time_range.ratio(time_range.start());

        // if another context is going to be created (in parallel) -
        // restore all removed elements.
        // self.tracks_tree.restore_removed();

        // TODO cut the unneeded tracks, load only needed.
        // if 0.0 <= split_ratio && split_ratio <= 1.0 {
        //     info! {
        //         target: LOG_TARGET,
        //         "replicate the context with a new time range"
        //     }

        //     let from = self.time_range.start().as_relative_time();
        //     let to = time_range.start().as_relative_time();

        //     let remove_area = mbr![t = [from; to]];

        //     self.tracks_tree.retain(&remove_area, |obj_space, id| {
        //         obj_space.get_data_mbr(id).bounds(0).max >= to
        //     });

        //     let tracks_space = self.tracks_tree.lock_obj_space().clone_shrinked();
        //     let tracks_tree = TracksTree::with_obj_space(tracks_space);

        //     let update_time_range = TimeRange::with_bounds(
        //         self.time_range.end(),
        //         time_range.end()
        //     );

        //     new_context = Self {
        //         session_id,
        //         layer_id,
        //         actors,
        //         actors_names,
        //         tracks_tree,
        //         time_range,
        //         new_objects: vec![],
        //     };

        //     (new_context, UpdateKind::Forward(update_time_range))
        // } else if -1.0 <= split_ratio && split_ratio <= 0.0 {
        //     info! {
        //         target: LOG_TARGET,
        //         "replicate the context with a new time range"
        //     }

        //     let from = time_range.end().as_relative_time();
        //     let to = self.time_range.end().as_relative_time();

        //     let remove_area = mbr![t = [from; to]];

        //     self.tracks_tree.retain(&remove_area, |obj_space, id| {
        //         obj_space.get_data_mbr(id).bounds(0).max >= from
        //     });

        //     let tracks_space = self.tracks_tree.lock_obj_space().clone_shrinked();
        //     let tracks_tree = TracksTree::with_obj_space(tracks_space);

        //     let update_time_range = TimeRange::with_bounds(
        //         time_range.start(),
        //         self.time_range.start()
        //     );

        //     new_context = Self {
        //         session_id,
        //         layer_id,
        //         actors,
        //         actors_names,
        //         tracks_tree,
        //         time_range,
        //         new_objects: vec![],
        //     };

        //     (new_context, UpdateKind::Backward(update_time_range))
        // } else {
            info! {
                target: LOG_TARGET,
                "create a new context with a new time range"
            }

            let tracks_tree = TracksTree::with_obj_space(Self::new_tracks_space());

            new_context = Self {
                session_id,
                layer_id,
                actors,
                actors_names,
                tracks_tree,
                time_range: time_range.clone(),
                new_objects: vec![],
            };

            (new_context, UpdateKind::Initial(time_range))
        // }
    }

    pub fn update_content(
        mut self,
        storage_mgr: StorageManager,
        update_kind: UpdateKind,
        interrupter: mpsc::Receiver<()>,
    ) -> Result<Self> {
        self.load_content_from_db(storage_mgr.clone(), update_kind)?;

        self.compute_tracks(storage_mgr, interrupter)?;

        Ok(self)
    }

    fn load_content_from_db(&mut self, storage_mgr: StorageManager, update_kind: UpdateKind) -> Result<()> {
        let mut connection = storage_mgr.pool.get()?;
        let reader = connection.copy_out(query![
            "COPY (
                    SELECT * FROM {schema_name}.current_objects_delta(
                        {layer_id},
                        ARRAY[{known_objects_ids}]::bigint[]
                    )
                ) TO stdout WITH (FORMAT CSV)",
            layer_id = self.layer_id,
            known_objects_ids = self.actors.iter().map(|(id, _)| id).join(",")
        ])?;

        let reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .from_reader(reader);

        self.load_objects_from_db(reader)?;

        let reader = connection.copy_out(query![
            "COPY
                    (
                        SELECT
                            out_location_id,
                            out_object_fk_id,
                            out_t,
                            out_x,
                            out_y,
                            out_z,
                            out_vx,
                            out_vy,
                            out_vz,

                            out_vcx,
                            out_vcy,
                            out_vcz

                            -- NULLIF(array_to_string(out_collision_partners, ','), '')
                        FROM
                            {schema_name}.range_locations({layer_id}, {start_time}, {stop_time}, {step_coeff})
                    )
                TO stdout WITH (FORMAT CSV)",
            layer_id = self.layer_id,
            start_time = update_kind.time_range().start().into_storage_duration(),
            stop_time = update_kind.time_range().end().into_storage_duration(),
            step_coeff = update_kind.as_step_coeff()
        ])?;

        let mut reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .from_reader(reader);

        // let mut collision_partners_map = HashMap::new();

        for result in reader.deserialize() {
            let location_info: LocationInfo = result.map_err(|err| Error::SerializeCSV(err))?;

            let object_id = location_info.object_id;
            let actor = self.actors.get_mut(&object_id).unwrap();
            match actor.last_gen_coord() {
                Some(last_coord) => {
                    actor.set_last_gen_coord(make_last_gen_coord(&location_info));

                    let time_range = TimeRange::with_bounds(last_coord.time(), location_info.t);

                    let _location_id = location_info.location_id;
                    // let collision_partners = location_info.collision_partners.clone();
                    let track_part_info =
                        make_track_part_info(object_id, last_coord, location_info);
                    let track_part_mbr =
                        make_track_part_mbr(
                            &time_range,
                            actor.object().radius(),
                            &track_part_info
                        );

                    let _track_part_id = self
                        .tracks_tree
                        .lock_obj_space_write()
                        .make_data_node(track_part_info, track_part_mbr);

                    // if !collision_partners.is_empty() {
                    //     collision_partners_map
                    //         .insert(location_id, (track_part_id, collision_partners));
                    // }
                }
                None => {
                    let initial_location = make_last_gen_coord(&location_info);
                    actor.set_last_gen_coord(initial_location);
                }
            }
        }

        // self.fix_collision_partners(collision_partners_map);

        self.rebuild_rtree();

        Ok(())
    }

    fn load_objects_from_db(
        &mut self,
        mut reader: csv::Reader<postgres::CopyOutReader>,
    ) -> Result<()> {
        for result in reader.deserialize() {
            let object_info: ObjectInfo = result.map_err(Error::SerializeCSV)?;

            let ObjectInfo(object_id, object) = object_info;
            self.new_objects.push(object_id);
            self.actors_names.insert(object.name().clone(), object_id);
            self.actors.insert(object_id, Actor::new(object));
        }

        Ok(())
    }

    // fn fix_collision_partners(
    //     &mut self,
    //     collision_partners_map: HashMap<LocationId, (TrackPartId, Vec<LocationId>)>,
    // ) {
    //     for (_, (track_part_id, db_partners_ids)) in collision_partners_map.iter() {
    //         self.tracks_tree
    //             .access_object_mut(*track_part_id, |track_part, _| {
    //                 let collision_partners_ids = db_partners_ids
    //                     .iter()
    //                     .map(|id| collision_partners_map.get(id).unwrap())
    //                     .map(|(track_part_id, _)| *track_part_id)
    //                     .collect();

    //                 track_part.collision_info.as_mut().unwrap().partners_ids =
    //                     collision_partners_ids
    //             })
    //     }
    // }

    fn compute_tracks(&mut self, storage_mgr: StorageManager, interrupter: mpsc::Receiver<()>) -> Result<()> {
        let mut checker = collision::CollisionChecker::new();

        let mut uncomputed = self.actors().keys().cloned().collect::<HashSet<_>>();
        while !uncomputed.is_empty() {
            let arc_checker = Arc::new(RwLock::new(checker));

            uncomputed = uncomputed.into_par_iter()
                .filter_map(|object_id| {
                    let mut checker = Arc::clone(&arc_checker);

                    let actor = self.actor(&object_id);
                    let last_coord = match actor.last_gen_coord() {
                        Some(coord) => coord,
                        None => return None,
                    };

                    let next_coord = next_gen_coord(
                        &last_coord,
                        actor.object().compute_step()
                    );

                    let track_part_info = TrackPartInfo::new(
                        object_id,
                        &last_coord,
                        &next_coord,
                        None
                    );

                    let time_range = TimeRange::with_bounds(
                        last_coord.time(),
                        next_coord.time()
                    );

                    let mbr = make_track_part_mbr(
                        &time_range,
                        actor.object().radius(),
                        &track_part_info
                    );

                    actor.set_last_gen_coord(next_coord);

                    self.tracks_tree().insert_transaction(
                        track_part_info,
                        mbr,
                        &mut checker
                    );

                    if time_range.end() > self.time_range().end() {
                        None
                    } else {
                        Some(object_id)
                    }
                })
                .collect();

            checker = Arc::try_unwrap(arc_checker).unwrap().into_inner().unwrap();

            let collisions = checker.collisions();

            let groups = petgraph::algo::tarjan_scc(&*collisions);
            let collision_info = groups.into_iter()
                .map(|group| {
                    find_collision_group(
                        self,
                        &checker,
                        group
                    )
                })
                .filter(|(_, group)| group.edge_count() != 0)
                .min_by(|(lhs_t, _), (rhs_t, _)| lhs_t.partial_cmp(&rhs_t).unwrap());

            if let Some((t, group)) = collision_info {
                let changed_ids = compute_collisions(self, t, group);

                uncomputed.extend(changed_ids);
            }

            checker.clear();

            if let Ok(_) = interrupter.try_recv() {
                info! {
                    target: LOG_TARGET,
                    "computing was interrupted"
                }

                return Err(Error::ContextUpdateInterrupted);
            }
        }

        let obj_space = self.tracks_tree.lock_obj_space().clone_shrinked();
        self.tracks_tree = LRTree::with_obj_space(obj_space);
        self.rebuild_rtree();

        self.update_db(storage_mgr)?;

        for (_, actor) in self.actors.iter_mut() {
            if let Some(last_coord) = actor.last_gen_coord() {
                actor.set_last_computed_time(last_coord.time());
            }
        }

        Ok(())
    }

    fn update_db(&self, storage_mgr: StorageManager) -> Result<()> {
        let mut connection = storage_mgr.pool.get()?;
        let writer = connection.copy_in(
            query!["COPY {schema_name}.location(
                object_fk_id,
                layer_fk_id,
                t,
                x,
                y,
                z,
                vx,
                vy,
                vz,
                vcx,
                vcy,
                vcz
            ) FROM stdin WITH (FORCE_NULL(vcx, vcy, vcz), FORMAT CSV)"]
        ).unwrap();

        let mut writer = csv::WriterBuilder::new()
            .quote_style(csv::QuoteStyle::NonNumeric)
            .from_writer(writer);

        let mbr = mbr![
            t = [self.time_range.start().as_relative_time(); self.time_range.end().as_relative_time()]
        ];

        self.tracks_tree.search_access(
            &mbr,
            |obj_space, id| {
                let mbr = obj_space.get_data_mbr(id);
                let track_part = obj_space.get_data_payload(id);

                let actor = self.actor(&track_part.object_id);

                let vc = match track_part.final_velocity {
                    Some(ref vel) => [
                            Some(vel[0]),
                            Some(vel[1]),
                            Some(vel[2])
                        ],
                    None => [None, None, None],
                };

                if actor.last_computed_time().as_relative_time() < mbr.bounds(0).max {
                    let location_info = LocationInfo {
                        object_id: track_part.object_id,
                        location_id: LocationId::default(),
                        layer_id: self.layer_id,
                        t: mbr.bounds(0).max.as_absolute_time(),
                        x: track_part.end_location[0],
                        y: track_part.end_location[1],
                        z: track_part.end_location[2],
                        vx: track_part.end_velocity[0],
                        vy: track_part.end_velocity[1],
                        vz: track_part.end_velocity[2],
                        vcx: vc[0],
                        vcy: vc[1],
                        vcz: vc[2],
                    };

                    writer.serialize(location_info).expect("update DB");
                }
            }
        );

        let writer = writer.into_inner().expect("unwrap CSV writer");
        writer.finish().expect("DB writer finish");

        Ok(())
    }

    fn rebuild_rtree(&self) {
        let alpha = 0.45;

        self.tracks_tree.rebuild(alpha);
    }

    fn new_tracks_space() -> TracksSpace {
        TracksSpace::new(GLOBAL_TREE_DIM, TREE_MIN_RECS, TREE_MAX_RECS)
    }
}

pub enum UpdateKind {
    Initial(TimeRange),
    // Forward(TimeRange),
    // Backward(TimeRange)
}

impl UpdateKind {
    fn as_step_coeff(&self) -> i16 {
        match self {
            UpdateKind::Initial(_) => 0,
            // UpdateKind::Forward(_) => 1,
            // UpdateKind::Backward(_) => -1,
        }
    }

    fn time_range(&self) -> &TimeRange {
        match self {
            UpdateKind::Initial(tr) => tr,
            // UpdateKind::Forward(tr) => tr,
            // UpdateKind::Backward(tr) => tr,
        }
    }
}