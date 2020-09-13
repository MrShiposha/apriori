use {
    std::{
        sync::{
            Arc,
            mpsc,
        },
        collections::{
            HashMap,
        },
    },
    lr_tree::*,
    crate::{
        Result,
        Error,
        r#type::{SessionId, LayerId, ObjectId, LocationId, ObjectName, Coord, AsRelativeTime, IntoStorageDuration},
        engine::actor::{Actor, TrackPartId, TrackPartsSpace},
        storage::StorageManager,
    },
    log::info,
    itertools::Itertools,
};

pub mod time_range;
mod db_util;

pub use {
    time_range::*,
};

use db_util::*;

use crate::query;

pub type ActorsMap = HashMap<ObjectId, Actor>;
pub type ActorsNamesMap = HashMap<ObjectName, ObjectId>;
pub type GlobalTracksSpace = ObjSpace<Coord, GlobalTrackPartInfo>;
pub type GlobalTracksTree = LRTree<Coord, GlobalTrackPartInfo>;
pub type GlobalTrackPartId = NodeId;

pub const GLOBAL_TREE_DIM: usize = 4;
pub const LOCAL_TREE_DIM: usize = 1;
pub const TREE_MIN_RECS: usize = 2;
pub const TREE_MAX_RECS: usize = 5;

const LOG_TARGET: &'static str = "context";

#[derive(Debug, Clone)]
pub struct GlobalTrackPartInfo {
    pub object_id: ObjectId,
    pub track_part_id: TrackPartId,
}

pub struct Context {
    session_id: SessionId,
    layer_id: LayerId,
    actors: ActorsMap,
    actors_names: ActorsNamesMap,
    tracks_tree: GlobalTracksTree,
    time_range: TimeRange,
    new_objects: Vec<ObjectId>,
}

impl Context {
    pub fn new(session_id: SessionId, layer_id: LayerId) -> Self {
        Self::with_time_range(
            session_id,
            layer_id,
            TimeRange::default(),
        )
    }

    pub fn with_time_range(session_id: SessionId, layer_id: LayerId, time_range: TimeRange) -> Self {
        Self {
            session_id,
            layer_id,
            actors: HashMap::new(),
            actors_names: HashMap::new(),
            tracks_tree: GlobalTracksTree::with_obj_space(
                Self::new_global_tracks_space()
            ),
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

    pub fn actor(&self, id: ObjectId) -> &Actor {
        self.actors.get(&id).unwrap()
    }

    pub fn actors(&self) -> &ActorsMap {
        &self.actors
    }

    pub fn take_new_object_id(&mut self) -> Option<ObjectId> {
        self.new_objects.pop()
    }

    pub fn replicate(
        &self,
        new_session_id: SessionId,
        new_layer_id: LayerId,
        new_time_range: TimeRange,
    ) -> Context {
        if new_session_id != self.session_id || new_layer_id != self.layer_id {
            info! {
                target: LOG_TARGET,
                "create a context on a new layer"
            }

            return Self::with_time_range(
                new_session_id,
                new_layer_id,
                new_time_range,
            );
        }

        macro_rules! retain_predicate {
            ($max_time:expr) => {
                |obj_space, id| {
                    obj_space.get_data_mbr(id).bounds(0).max >= $max_time
                }
            };
        }

        let session_id = self.session_id;
        let layer_id = self.layer_id;
        let actors_names = self.actors_names.clone();
        let time_range = new_time_range;

        if self.time_range.contains(time_range.start()) {
            info! {
                target: LOG_TARGET,
                "replicate the context with a new time range"
            }

            // if another context is going to be created (in parallel) -
            // restore all removed elements.
            self.tracks_tree.restore_removed();

            let from = self.time_range.start().as_relative_time();
            let to = time_range.start().as_relative_time();

            let remove_area = mbr![t = [from; to]];

            self.tracks_tree.retain(
                &remove_area,
                retain_predicate![to]
            );

            self.actors.iter().for_each(|(_, actor)| {
                // if another context is going to be created (in parallel) -
                // restore all removed elements.
                actor.track_parts_tree().restore_removed();

                actor.track_parts_tree()
                    .retain(&remove_area, retain_predicate![to])
            });

            let actors = self.actors.clone();

            let tracks_space = self.tracks_tree.lock_obj_space().clone_shrinked();
            let tracks_tree = GlobalTracksTree::with_obj_space(tracks_space);

            Self {
                session_id,
                layer_id,
                actors,
                actors_names,
                tracks_tree,
                time_range,
                new_objects: vec![],
            }
        } else {
            info! {
                target: LOG_TARGET,
                "create a new context with a new time range"
            }

            let actors = self.actors.iter()
                .map(|(id, actor)| {
                    let actor = Actor::new(
                        actor.object().clone(),
                        Self::new_tracks_space()
                    );

                    (*id, actor)
                })
                .collect();

            let tracks_tree = GlobalTracksTree::with_obj_space(
                Self::new_global_tracks_space()
            );

            Self {
                session_id,
                layer_id,
                actors,
                actors_names,
                tracks_tree,
                time_range,
                new_objects: vec![],
            }
        }
    }

    pub fn update_content(
        mut self,
        storage_mgr: StorageManager,
        interrupter: mpsc::Receiver<()>
    ) -> Result<Self> {
        self.load_content_from_db(storage_mgr)?;

        let arc_self = Arc::new(self);

        // TODO write new objects' locations into the DB

        Arc::try_unwrap(arc_self)
            .map_err(|_| Error::ContextUpdateInterrupted)
    }

    fn load_content_from_db(&mut self, storage_mgr: StorageManager) -> Result<()> {
        let mut connection = storage_mgr.pool.get()?;
        let reader = connection.copy_out(
            query![
                "COPY (
                    SELECT * FROM {schema_name}.current_objects_delta(
                        {layer_id},
                        ARRAY[{known_objects_ids}]::bigint[]
                    )
                ) TO stdout WITH (FORMAT CSV)",
                layer_id = self.layer_id,
                known_objects_ids = self.actors.iter().map(|(id, _)| id).join(",")
            ]
        )?;

        let reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .from_reader(reader);

        self.load_objects_from_db(reader)?;

        let reader = connection.copy_out(
            query![
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
                            out_vcz,

                            NULLIF(array_to_string(out_collision_partners, ','), '')
                        FROM
                            {schema_name}.range_locations({layer_id}, {start_time}, {stop_time})
                    )
                TO stdout WITH (FORMAT CSV)",
                layer_id = self.layer_id,
                start_time = self.time_range.start().into_storage_duration(),
                stop_time = self.time_range.end().into_storage_duration()
            ]
        )?;

        let mut reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .from_reader(reader);

        let mut collision_partners_map = HashMap::new();

        for result in reader.deserialize() {
            let location_info: LocationInfo = result
                .map_err(|err| Error::SerializeCSV(err))?;

            let object_id = location_info.object_id;
            let actor = self.actors.get_mut(&object_id).unwrap();
            match actor.last_gen_coord() {
                Some(last_coord) => {
                    actor.set_last_location(make_gen_coord(&location_info));

                    let time_range = TimeRange::with_bounds(
                        last_coord.time(),
                        location_info.t,
                    );

                    let location_id = location_info.location_id;
                    let collision_partners = location_info.collision_partners.clone();
                    let track_part_info = make_track_part_info(last_coord, location_info);
                    let global_mbr = make_global_mbr(
                        &time_range,
                        actor.object().radius(),
                        &track_part_info
                    );

                    let track_part_id = actor.add_track_part_unchecked(&time_range, track_part_info);

                    self.tracks_tree.lock_obj_space_write()
                        .make_data_node(
                            GlobalTrackPartInfo {
                                object_id,
                                track_part_id
                            },
                            global_mbr
                        );

                    if !collision_partners.is_empty() {
                        collision_partners_map.insert(
                            location_id,
                            (
                                object_id,
                                track_part_id,
                                collision_partners
                            )
                        );
                    }
                },
                None => {
                    let initial_location = make_gen_coord(&location_info);
                    actor.set_last_location(initial_location);
                }
            }
        }

        self.fix_collision_partners(collision_partners_map);

        self.rebuild_rtrees();

        Ok(())
    }

    fn load_objects_from_db(&mut self, mut reader: csv::Reader<postgres::CopyOutReader>) -> Result<()> {
        for result in reader.deserialize() {
            let object_info: ObjectInfo = result
                .map_err(Error::SerializeCSV)?;

            let ObjectInfo(object_id, object) = object_info;
            self.new_objects.push(object_id);
            self.actors_names.insert(object.name().clone(), object_id);
            self.actors.insert(
                object_id,
                Actor::new(
                    object,
                    Self::new_tracks_space()
                )
            );
        }

        Ok(())
    }

    fn fix_collision_partners(
        &mut self,
        collision_partners_map: HashMap<LocationId, (ObjectId, TrackPartId, Vec<LocationId>)>
    ) {
        for (_, (object_id, track_part_id, db_partners_id)) in collision_partners_map.iter() {
            self.actors.get(object_id)
                .unwrap()
                .track_parts_tree()
                .access_object_mut(
                    *track_part_id,
                    |track_part, _| {
                        let collision_partners_ids = db_partners_id
                            .iter()
                            .map(|id| collision_partners_map.get(id).unwrap())
                            .map(|(object_id, track_part_id, _)| {
                                (*object_id, *track_part_id)
                            })
                            .collect();

                        track_part.collision_info
                            .as_mut()
                            .unwrap()
                            .partners_ids = collision_partners_ids
                    }
                )
        }
    }

    fn rebuild_rtrees(&self) {
        let alpha = 0.45;

        self.tracks_tree.rebuild(alpha);
        for (_, actor) in self.actors.iter() {
            actor.track_parts_tree().rebuild(alpha);
        }
    }

    fn new_tracks_space() -> TrackPartsSpace {
        TrackPartsSpace::new(
            LOCAL_TREE_DIM,
            TREE_MIN_RECS,
            TREE_MAX_RECS
        )
    }

    fn new_global_tracks_space() -> GlobalTracksSpace {
        GlobalTracksSpace::new(
            GLOBAL_TREE_DIM,
            TREE_MIN_RECS,
            TREE_MAX_RECS,
        )
    }
}