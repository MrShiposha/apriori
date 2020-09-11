use {
    std::{
        collections::HashMap,
    },
    lr_tree::*,
    crate::{
        r#type::{ObjectId, ObjectName, Coord, AsRelativeTime},
        engine::actor::{Actor, TrackPartId},
    },
    log::info,
};

pub const GLOBAL_TREE_DIM: usize = 4;
pub const LOCAL_TREE_DIM: usize = 1;
pub const TREE_MIN_RECS: usize = 2;
pub const TREE_MAX_RECS: usize = 5;

const LOG_TARGET: &'static str = "context";

#[derive(Debug, Clone)]
pub struct TrackPartInfo {
    pub object_id: ObjectId,
    pub track_part_id: TrackPartId,
}

pub type TracksSpace = ObjSpace<Coord, TrackPartInfo>;
pub type TracksTree = LRTree<Coord, TrackPartInfo>;

pub struct Context {
    actors: HashMap<ObjectId, Actor>,
    actors_names: HashMap<ObjectName, ObjectId>,
    tracks_tree: TracksTree,
    time_range: TimeRange,
    is_poisoned: bool,
}

impl Context {
    pub fn new() -> Self {
        let default_len = chrono::Duration::seconds(10);

        Self {
            actors: HashMap::new(),
            actors_names: HashMap::new(),
            tracks_tree: TracksTree::with_obj_space(
                TracksSpace::new(
                    GLOBAL_TREE_DIM,
                    TREE_MIN_RECS,
                    TREE_MAX_RECS,
                )
            ),
            time_range: TimeRange::new(chrono::Duration::zero(), default_len),
            is_poisoned: false
        }
    }

    pub fn is_empty(&self) -> bool {
        self.actors.is_empty()
    }

    pub fn update_time_range(&mut self, time_range: TimeRange) {
        macro_rules! retain_predicate {
            ($max_time:expr) => {
                |obj_space, id| {
                    obj_space.get_data_mbr(id).bounds(0).max >= $max_time
                }
            };
        }

        info! {
            target: LOG_TARGET,
            "update context time range"
        }

        let ratio = self.time_range.ratio(time_range.start);

        if 0.0 <= ratio && ratio <= 1.0 {
            info! {
                target: LOG_TARGET,
                "new start time is in the old range, free unneded trees' nodes"
            }

            let from = self.time_range.start.as_relative_time();
            let to = time_range.start.as_relative_time();

            let remove_area = mbr![t = [from; to]];

            self.tracks_tree.retain(
                &remove_area,
                retain_predicate![to]
            );

            self.actors.iter().for_each(|(_, actor)| {
                actor.track_parts_tree()
                    .retain(&remove_area, retain_predicate![to])
            });
        } else {
            info! {
                target: LOG_TARGET,
                "new start time is out of old range, the context is poisoned"
            }

            self.is_poisoned = true;
        }
    }
}

impl Clone for Context {
    fn clone(&self) -> Self {
        if self.is_poisoned {
            Self::new()
        } else {
            let actors = self.actors.clone();
            let actors_names = self.actors_names.clone();
            let time_range = self.time_range.clone();

            let tracks_space = self.tracks_tree.lock_obj_space().clone_shrinked();
            let tracks_tree = TracksTree::with_obj_space(tracks_space);

            Self {
                actors,
                actors_names,
                tracks_tree,
                time_range,
                is_poisoned: false
            }
        }
    }
}

#[derive(Clone)]
pub struct TimeRange {
    start: chrono::Duration,
    end: chrono::Duration,
}

impl TimeRange {
    pub fn new(start: chrono::Duration, length: chrono::Duration) -> Self {
        let end = start + length;

        assert!(start < end);

        Self {
            start,
            end
        }
    }

    pub fn start(&self) -> chrono::Duration {
        self.start
    }

    pub fn end(&self) -> chrono::Duration {
        self.end
    }

    pub fn length(&self) -> chrono::Duration {
        self.end - self.start
    }

    pub fn offset(&self, vtime: chrono::Duration) -> chrono::Duration {
        vtime - self.start
    }

    pub fn ratio(&self, vtime: chrono::Duration) -> f32 {
        self.offset(vtime).as_relative_time() / self.length().as_relative_time()
    }
}