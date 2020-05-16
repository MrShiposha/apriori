use {
    std::ops::{
        Range,
        RangeBounds,
    },
    crate::{
        make_error,
        r#type::{
            Vector,
            TimeDirection,
            RawTime,
            RelativeTime,
            AsRelativeTime,
            AsAbsoluteTime,
        },
        math::hermite_interpolation,
        scene::{
            ringbuffer::{
                self,
                RingBuffer,
            },
            Object4d, 
            TruncateRange
        },
        shared::SharedWeak,
        Result,
    },
};

pub type CanceledCollisions = Vec<Collision>;

#[derive(Clone)]
pub struct TrackAtom {
    location: Vector,
    velocity: Vector,
    step: chrono::Duration,
}

impl TrackAtom {
    pub fn new(location: Vector, velocity: Vector) -> Self {
        Self {
            location,
            velocity,
            step: chrono::Duration::zero(),
        }
    }

    pub fn with_location(location: Vector) -> Self {
        Self {
            location,
            velocity: Vector::zeros(),
            step: chrono::Duration::zero(),
        }
    }

    pub fn location(&self) -> &Vector {
        &self.location
    }

    pub fn velocity(&self) -> &Vector {
        &self.velocity
    }

    pub fn at_next_location(&self, step: RelativeTime) -> TrackAtom {
        TrackAtom {
            location: self.location + self.velocity * step,
            velocity: self.velocity,
            step: chrono::Duration::zero(),
        }
    }

    pub fn set_velocity(&mut self, velocity: Vector) {
        self.velocity = velocity;
    }
}

impl Default for TrackAtom {
    fn default() -> Self {
        Self {
            location: Vector::zeros(),
            velocity: Vector::zeros(),
            step: chrono::Duration::zero(),
        }
    }
}

pub struct SpaceTimeAtom<'atom> {
    pub track_atom: &'atom TrackAtom,
    pub time: RelativeTime,
}

impl SpaceTimeAtom<'_> {
    pub fn new<'atom>(
        track_atom: &'atom TrackAtom,
        time: RelativeTime,
    ) -> SpaceTimeAtom<'atom> {
        SpaceTimeAtom::<'atom> { track_atom, time }
    }
}

#[derive(Clone)]
pub struct Collision {
    pub colliding_object: SharedWeak<Object4d>,
    pub when: chrono::Duration,
    pub step: chrono::Duration,
    pub back_step: chrono::Duration,
    pub time_direction: TimeDirection,
    pub src_track_atom: TrackAtom,
    pub track_atom: TrackAtom,
}

impl Collision {
    pub fn new(
        colliding_object: SharedWeak<Object4d>,
        when: chrono::Duration,
        time_direction: TimeDirection,
        src_track_atom: TrackAtom,
        track_atom: TrackAtom,
    ) -> Self {
        Self {
            colliding_object,
            when,
            step: chrono::Duration::zero(),
            back_step: chrono::Duration::zero(),
            time_direction,
            src_track_atom,
            track_atom,
        }
    }

    pub fn at_next_location(&self, step: RelativeTime) -> TrackAtom {
        TrackAtom {
            location: self.track_atom.location + self.track_atom.velocity * step,
            velocity: self.track_atom.velocity,
            step: chrono::Duration::zero(),
        }
    }
}

#[derive(Clone)]
pub enum TrackNode {
    Atom(TrackAtom),
    Collision(Collision),
}

impl TrackNode {
    pub fn location(&self) -> &Vector {
        match self {
            TrackNode::Atom(atom) => atom.location(),
            TrackNode::Collision(collision) => collision.track_atom.location(),
        }
    }

    pub fn velocity(&self) -> &Vector {
        match self {
            TrackNode::Atom(atom) => atom.velocity(),
            TrackNode::Collision(collision) => collision.track_atom.velocity(),
        }
    }

    pub fn at_next_location(&self, step: RelativeTime) -> TrackAtom {
        match self {
            TrackNode::Atom(atom) => atom.at_next_location(step),
            TrackNode::Collision(collision) => collision.at_next_location(step),
        }
    }

    pub fn step(&self) -> chrono::Duration {
        match self {
            TrackNode::Atom(atom) => atom.step,
            TrackNode::Collision(collision) => collision.step,
        }
    }

    pub fn set_step(&mut self, step: chrono::Duration) {
        match self {
            TrackNode::Atom(atom) => atom.step = step,
            TrackNode::Collision(collision) => collision.step = step,
        }
    }
}

impl Default for TrackNode {
    fn default() -> Self {
        Self::Atom(TrackAtom::default())
    }
}

impl From<TrackAtom> for TrackNode {
    fn from(atom: TrackAtom) -> Self {
        Self::Atom(atom)
    }
}

impl From<Collision> for TrackNode {
    fn from(collision:Collision) -> Self {
        Self::Collision(collision)
    }
}

pub struct Track {
    nodes: RingBuffer<TrackNode>,
    time_start: chrono::Duration,
    time_end: chrono::Duration,
    compute_step: chrono::Duration,
}

impl Track {
    pub fn new(track_size: usize, compute_step: chrono::Duration) -> Self {
        Self {
            nodes: RingBuffer::new(track_size),
            time_start: chrono::Duration::zero(),
            time_end: chrono::Duration::zero(),
            compute_step,
        }
    }

    pub fn interpolate(&self, vtime: &chrono::Duration) -> Result<Vector> {
        let computed_range = self.computed_range();
        if !computed_range.contains(vtime) {
            return Err(make_error![Error::Scene::UncomputedTrackPart(*vtime, computed_range)]);
        }

        let time_offset = self.time_offset(vtime);

        let (lhs_index, lhs_time) = self.node_position(time_offset);
        let lhs = &self.nodes[lhs_index];
        let rhs = &self.nodes[lhs_index + 1];
        let location = Self::interpolate_nodes(
            time_offset, 
            lhs_time, 
            lhs, 
            rhs
        );

        Ok(location)
    }

    fn interpolate_nodes(
        time_offset: chrono::Duration, 
        lhs_time: chrono::Duration,
        lhs: &TrackNode, 
        rhs: &TrackNode
    ) -> Vector {
        let rhs_time = lhs_time + lhs.step();

        let time_offset = time_offset.as_relative_time();
        let lhs_time = lhs_time.as_relative_time();
        let rhs_time = rhs_time.as_relative_time();

        match lhs {
            TrackNode::Atom(lhs) => match rhs {
                TrackNode::Atom(rhs) => interpolate_track_part(
                    SpaceTimeAtom::new(lhs, lhs_time),
                    SpaceTimeAtom::new(rhs, rhs_time),
                    time_offset,
                ),
                TrackNode::Collision(rhs) => interpolate_track_part(
                    SpaceTimeAtom::new(lhs, lhs_time),
                    SpaceTimeAtom::new(&rhs.src_track_atom, rhs_time), 
                    time_offset,
                )
            },
            TrackNode::Collision(lhs) => match rhs {
                TrackNode::Atom(rhs) => interpolate_track_part(
                    SpaceTimeAtom::new(&lhs.track_atom, lhs_time), 
                    SpaceTimeAtom::new(rhs, rhs_time), 
                    time_offset,
                ),
                TrackNode::Collision(rhs) => interpolate_track_part(
                    SpaceTimeAtom::new(&lhs.track_atom, lhs_time), 
                    SpaceTimeAtom::new(&rhs.src_track_atom, rhs_time), 
                    time_offset,
                )
            }
        }
    }

    pub fn compute_step(&self) -> chrono::Duration {
        self.compute_step
    }

    pub fn computed_range(&self) -> Range<chrono::Duration> {
        Range::<chrono::Duration> {
            start: self.time_start(),
            end: self.time_end(),
        }
    }

    pub fn time_start(&self) -> chrono::Duration {
        self.time_start
    }

    pub fn time_end(&self) -> chrono::Duration {
        self.time_end
    }

    pub fn time_length(&self) -> chrono::Duration {
        self.time_end - self.time_start
    }

    pub fn node_start(&self) -> &TrackNode {
        self.nodes.first().unwrap()
    }

    pub fn node_start_mut(&mut self) -> &mut TrackNode {
        self.nodes.first_mut().unwrap()
    }

    pub fn node_end(&self) -> &TrackNode {
        self.nodes.last().unwrap()
    }

    pub fn node_end_mut(&mut self) -> &mut TrackNode {
        self.nodes.last_mut().unwrap()
    }

    pub fn set_initial_node(&mut self, mut node: TrackAtom, time_start: chrono::Duration) {
        self.nodes.clear();
        self.time_start = time_start;
        self.time_end = time_start;
        
        node.step = self.compute_step;
        self.nodes.push_back(node.into());
    }

    pub fn push_back(&mut self, mut node: TrackNode) {
        let time_start = self.time_start;
        let old_node = self.node_end();
        let mut old_step = old_node.step();

        match node {
            TrackNode::Atom(ref mut new) => new.step = self.compute_step,
            TrackNode::Collision(ref mut new) => {
                new.back_step = (new.when - time_start) - self.nearest_compute_step_time(&new.when);

                let old_node_step = match old_node {
                    TrackNode::Atom(_) => {
                        new.back_step
                    },
                    TrackNode::Collision(old) => {
                        new.when - old.when
                    }
                };

                let old_node = self.node_end_mut();
                old_node.set_step(old_node_step);

                new.step = old_step - old_node.step();
                old_step = old_node.step();
            }
        }

        self.time_end = self.time_end + old_step;

        if let Some(removed) = self.nodes.push_back(node) {
            self.time_start = self.time_start + removed.step();
        }
    }

    pub fn push_front(&mut self, mut node: TrackNode) {
        let old_node = self.node_start();

        match node {
            TrackNode::Atom(ref mut new) => {
                match old_node {
                    TrackNode::Atom(_) => new.step = self.compute_step,
                    TrackNode::Collision(old) => new.step = old.back_step,
                }

                self.time_start = self.time_start - new.step;
            },
            TrackNode::Collision(ref mut new) => {
                match old_node {
                    TrackNode::Atom(_) => {
                        new.step = self.time_start - new.when;
                        new.back_step = self.compute_step - new.step;
                    },
                    TrackNode::Collision(old) => {
                        new.step = old.when - new.when;
                        new.back_step = old.back_step - new.step;
                    }
                }

                self.time_start = new.when;
            }
        }

        if let Some(_) = self.nodes.push_front(node) {
            self.time_end = self.time_end - self.node_end().step();
        }
    }

    pub fn place_collision(
        &mut self,
        colliding_object: SharedWeak<Object4d>,
        when: chrono::Duration,
        time_direction: TimeDirection,
        track_atom: TrackAtom,
    ) -> CanceledCollisions {
        let (collision, canceled) = self.make_new_collision(
            colliding_object, 
            when, 
            time_direction, 
            track_atom
        );

        match collision.time_direction {
            TimeDirection::Forward => self.push_back(collision.into()),
            TimeDirection::Backward => self.push_front(collision.into()),
        }

        canceled
    }

    pub fn make_new_collision<'track>(
        &'track mut self,
        colliding_object: SharedWeak<Object4d>,
        when: chrono::Duration,
        time_direction: TimeDirection,
        track_atom: TrackAtom,
    ) -> (Collision, CanceledCollisions) {
        let peek_src_node: fn(&mut ringbuffer::Truncated<'track, TrackNode>) 
            -> Option<<ringbuffer::Truncated<'track, TrackNode> as Iterator>::Item>;
        let range;
 
        match time_direction {
            TimeDirection::Forward => {
                peek_src_node = ringbuffer::Truncated::<'track, TrackNode>::peek_first;
                range = TruncateRange::From(when);
            },
            TimeDirection::Backward => {
                peek_src_node = ringbuffer::Truncated::<'track, TrackNode>::peek_last;
                range = TruncateRange::To(when);
            }
        }

        let mut canceled = self.truncate(range);
        let src_node = peek_src_node(&mut canceled).expect("canceled track can't be empty");
        let src_atom = TrackAtom::new(*src_node.location(), *src_node.velocity());

        let collision = Collision::new(
            colliding_object, 
            when, 
            time_direction, 
            src_atom, 
            track_atom
        );

        let canceled = canceled.filter_map(|node| match node {
            TrackNode::Atom(_) => None,
            TrackNode::Collision(node) => Some(node.clone())
        }).collect();


        (collision, canceled)
    }

    pub fn iter_nodes(&self) -> ringbuffer::Iter<TrackNode> {
        self.nodes.iter()
    }

    pub fn is_fully_computed(&self) -> bool {
        self.nodes.len() == self.nodes.capacity()
    }

    pub fn truncate(
        &mut self, 
        range: impl Into<TruncateRange<chrono::Duration>>
    ) -> ringbuffer::Truncated<TrackNode> {
        let range = range.into();
        let range = range.map(|time| self.node_position(self.time_offset(time)));
        
        let canceled = self.nodes.truncate(
            range.map(|(node_index, _)| *node_index)
        );

        match range {
            TruncateRange::From((_, time)) => self.time_end = time + self.time_start,
            TruncateRange::To((_, time)) => self.time_start = time + self.time_start,
        }

        canceled
    }

    fn node_position(&self, time_offset: chrono::Duration) -> (usize, chrono::Duration) {
        let mut index = (
            time_offset.num_milliseconds() / 
            self.compute_step.num_milliseconds()
        ) as usize;

        let mut index_time = self.compute_step * index as i32;

        let mut index_step = self.nodes[index].step();
        while time_offset > index_time + index_step {
            index += 1;
            index_time = index_time + index_step;
            index_step = self.nodes[index].step();
        }

        (index, index_time)
    }

    fn nearest_compute_step_time(&self, vtime: &chrono::Duration) -> chrono::Duration {
        let index = (
            self.time_offset(vtime).num_milliseconds() / 
            self.compute_step.num_milliseconds()
        ) as usize;

        self.compute_step * index as i32
    }

    pub fn time_offset(&self, vtime: &chrono::Duration) -> chrono::Duration {
        *vtime - self.time_start
    }
}

fn interpolate_track_part(
    lhs: SpaceTimeAtom,
    rhs: SpaceTimeAtom,
    vtime: RelativeTime,
) -> Vector {
    hermite_interpolation(
        &lhs.track_atom.location,
        &lhs.track_atom.velocity,
        lhs.time,
        
        &rhs.track_atom.location,
        &rhs.track_atom.velocity,
        rhs.time,
        vtime,
    )
}