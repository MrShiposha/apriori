use {
    crate::{
        scene::{
            track::{
                Track,
                TrackNode,
                TrackAtom,
            },
        },
        r#type::{
            RelativeTime,
            AsRelativeTime,
        },
        shared::Shared,
    }
};

pub trait UncomputedTrack {
    fn add_node(track: &mut Track, node: TrackNode);

    fn last_node(track: &Track) -> Shared<TrackNode>;

    fn last_atom(last_node: &TrackNode) -> &TrackAtom;

    fn last_time(track: &Track) -> RelativeTime;

    fn time_step(track: &Track) -> RelativeTime;

    fn new_time(track: &Track) -> RelativeTime {
        Self::last_time(track) + Self::time_step(track)
    }
}

pub struct ForwardUncomputedTrack;
pub struct BackwardUncomputedTrack;

impl UncomputedTrack for ForwardUncomputedTrack {
    fn add_node(track: &mut Track, node: TrackNode) {
        track.push_back(node);
    }

    fn last_node(track: &Track) -> Shared<TrackNode> {
        track.node_end()
    }

    fn last_atom(last_node: &TrackNode) -> &TrackAtom {
        last_node.atom_end()
    }

    fn last_time(track: &Track) -> RelativeTime {
        track.time_end().as_relative_time()
    }

    fn time_step(track: &Track) -> RelativeTime {
        track.relative_compute_step()
    }
}

impl UncomputedTrack for BackwardUncomputedTrack {
    fn add_node(track: &mut Track, node: TrackNode) {
        track.push_front(node);
    }

    fn last_node(track: &Track) -> Shared<TrackNode> {
        track.node_start()
    }

    fn last_atom(last_node: &TrackNode) -> &TrackAtom {
        last_node.atom_start()
    }

    fn last_time(track: &Track) -> RelativeTime {
        track.time_start().as_relative_time()
    }

    fn time_step(track: &Track) -> RelativeTime {
        -track.relative_compute_step()
    }

    fn new_time(track: &Track) -> RelativeTime {
        let step = track.relative_compute_step();
        Self::last_time(track) - step
    }
}