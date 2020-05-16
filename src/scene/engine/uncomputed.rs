use {
    crate::{
        scene::{
            track::{
                Track,
                TrackNode,
            },
        },
        r#type::{
            RelativeTime,
            AsRelativeTime,
            TimeDirection,
        },
    }
};

pub trait UncomputedTrack {
    fn add_node(track: &mut Track, node: TrackNode);

    fn last_node(track: &Track) -> &TrackNode;

    fn last_node_mut(track: &mut Track) -> &mut TrackNode;

    fn last_time(track: &Track) -> RelativeTime;

    fn time_step(track: &Track) -> RelativeTime;

    fn new_time(track: &Track) -> RelativeTime {
        Self::last_time(track) + Self::time_step(track)
    }

    fn time_direction() -> TimeDirection;
}

pub struct ForwardUncomputedTrack;
pub struct BackwardUncomputedTrack;

impl UncomputedTrack for ForwardUncomputedTrack {
    fn add_node(track: &mut Track, node: TrackNode) {
        track.push_back(node);
    }

    fn last_node(track: &Track) -> &TrackNode {
        track.node_end()
    }

    fn last_node_mut(track: &mut Track) -> &mut TrackNode {
        track.node_end_mut()
    }

    fn last_time(track: &Track) -> RelativeTime {
        track.time_end().as_relative_time()
    }

    fn time_step(track: &Track) -> RelativeTime {
        Self::last_node(track).step().as_relative_time()
    }

    fn time_direction() -> TimeDirection {
        TimeDirection::Forward
    }
}

impl UncomputedTrack for BackwardUncomputedTrack {
    fn add_node(track: &mut Track, node: TrackNode) {
        track.push_front(node);
    }

    fn last_node(track: &Track) -> &TrackNode {
        track.node_start()
    }

    fn last_node_mut(track: &mut Track) -> &mut TrackNode {
        track.node_start_mut()
    }

    fn last_time(track: &Track) -> RelativeTime {
        track.time_start().as_relative_time()
    }

    fn time_step(track: &Track) -> RelativeTime {
        match Self::last_node(track) {
            TrackNode::Atom(_) => -track.compute_step().as_relative_time(),
            TrackNode::Collision(node) => -node.back_step.as_relative_time(),
        }
    }

    fn new_time(track: &Track) -> RelativeTime {
        Self::last_time(track) - Self::time_step(track)
    }

    fn time_direction() -> TimeDirection {
        TimeDirection::Backward
    }
}