use {
    std::{
        ops::Range,
        sync::{Mutex, Weak},
    },
    crate::{
        make_error,
        r#type::{
            Vector,
            RawTime,
        },
        math::hermite_interpolation,
        scene::{ringbuffer::RingBuffer, Object4d, TruncateRange},
        shared::Shared,
        Result,
    },
};

pub struct TrackAtom {
    location: Vector,
    velocity: Vector,
}

impl TrackAtom {
    pub fn new(location: Vector, velocity: Vector) -> Self {
        Self {
            location,
            velocity
        }
    }

    pub fn with_location(location: Vector) -> Self {
        Self {
            location,
            velocity: Vector::zeros()
        }
    }

    pub fn location(&self) -> &Vector {
        &self.location
    }

    pub fn velocity(&self) -> &Vector {
        &self.velocity
    }

    pub fn at_next_location(&self, step: f32) -> TrackAtom {
        TrackAtom {
            location: self.location + self.velocity * step,
            velocity: self.velocity
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
        }
    }
}

pub struct SpaceTimeAtom<'atom> {
    pub track_atom: &'atom TrackAtom,
    pub time: &'atom chrono::Duration,
}

impl SpaceTimeAtom<'_> {
    pub fn new<'atom>(
        track_atom: &'atom TrackAtom,
        time: &'atom chrono::Duration,
    ) -> SpaceTimeAtom<'atom> {
        SpaceTimeAtom::<'atom> { track_atom, time }
    }
}

pub enum Composite {
    Collision(CollisionList),
}

impl Composite {
    fn atom_start(&self) -> &TrackAtom {
        match self {
            Composite::Collision(list) => &list.atom_start,
        }
    }

    fn atom_end(&self) -> &TrackAtom {
        match self {
            Composite::Collision(list) => &list.collisions.last().unwrap().track_atom,
        }
    }

    fn time_start(&self) -> &chrono::Duration {
        match self {
            Composite::Collision(list) => &list.begin_time,
        }
    }

    fn time_end(&self) -> &chrono::Duration {
        match self {
            Composite::Collision(list) => &list.collisions.last().unwrap().when,
        }
    }

    fn contains_time(&self, vtime: &chrono::Duration) -> bool {
        (self.time_start()..self.time_end()).contains(&vtime)
    }

    fn interpolate(&self, vtime: &chrono::Duration) -> Vector {
        match self {
            Composite::Collision(list) => list.interpolate(vtime),
        }
    }
}

pub enum TrackNode {
    Atom(TrackAtom),
    Composite(Composite),
}

impl TrackNode {
    pub fn atom_start(&self) -> &TrackAtom {
        match self {
            TrackNode::Atom(atom) => atom,
            TrackNode::Composite(composite) => composite.atom_start(),
        }
    }

    pub fn atom_end(&self) -> &TrackAtom {
        match self {
            TrackNode::Atom(atom) => atom,
            TrackNode::Composite(composite) => composite.atom_end(),
        }
    }

    pub fn time_start(&self) -> Option<&chrono::Duration> {
        match self {
            TrackNode::Atom(_) => None,
            TrackNode::Composite(composite) => Some(composite.time_start()),
        }
    }

    pub fn time_end(&self) -> Option<&chrono::Duration> {
        match self {
            TrackNode::Atom(_) => None,
            TrackNode::Composite(composite) => Some(composite.time_end()),
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

pub struct Collision {
    with: Weak<Mutex<Object4d>>,
    when: chrono::Duration,
    track_atom: TrackAtom,
}

pub struct CollisionList {
    atom_start: TrackAtom,
    begin_time: chrono::Duration,
    collisions: Vec<Collision>,
}

impl Default for CollisionList {
    fn default() -> Self {
        Self {
            atom_start: TrackAtom::default(),
            begin_time: chrono::Duration::zero(),
            collisions: vec![],
        }
    }
}

impl CollisionList {
    fn interpolate(&self, vtime: &chrono::Duration) -> Vector {
        let first_collision = self.collisions.first().unwrap();
        if *vtime < first_collision.when {
            interpolate_track_part(
                SpaceTimeAtom::new(&self.atom_start, &self.begin_time),
                SpaceTimeAtom::new(&first_collision.track_atom, &first_collision.when),
                vtime,
            )
        } else {
            let node_index = self
                .collisions
                .binary_search_by_key(vtime, |collision| collision.when)
                .unwrap();

            let lhs = &self.collisions[node_index];
            let rhs = &self.collisions[node_index + 1];

            interpolate_track_part(
                SpaceTimeAtom::new(&lhs.track_atom, &lhs.when),
                SpaceTimeAtom::new(&rhs.track_atom, &rhs.when),
                vtime,
            )
        }
    }
}

pub struct Track {
    nodes: RingBuffer<Shared<TrackNode>>,
    time_start: chrono::Duration,
    compute_step: chrono::Duration,
}

impl Track {
    pub fn new(track_size: usize, compute_step: chrono::Duration) -> Self {
        Self {
            nodes: RingBuffer::new(track_size),
            time_start: chrono::Duration::zero(),
            compute_step,
        }
    }

    pub fn interpolate(&self, vtime: &chrono::Duration) -> Result<Vector> {
        if !self.computed_range().contains(vtime) {
            return Err(make_error![Error::Scene::UncomputedTrackPart(*vtime)]);
        }

        let relative_time = self.track_relative_time(vtime);
        let node_index = self.track_node_index(vtime);
        let lhs_node_time = chrono::Duration::milliseconds(
            self.compute_step.num_milliseconds() * node_index as RawTime
        ) + self.time_start;

        let rhs_node_time = lhs_node_time + self.compute_step;

        let node = &*self.nodes[node_index].read().unwrap();
        let interpolated = match node {
            TrackNode::Atom(lhs) => {
                let rhs = &*self.nodes[node_index + 1].read().unwrap();

                interpolate_track_part(
                    SpaceTimeAtom::new(lhs, &lhs_node_time),
                    SpaceTimeAtom::new(rhs.atom_start(), &rhs_node_time),
                    &vtime,
                )
            }
            TrackNode::Composite(composite) => {
                if composite.contains_time(vtime) {
                    composite.interpolate(vtime)
                } else {
                    let lhs = composite;
                    let rhs = self.nodes[node_index + 1].read().unwrap();

                    interpolate_track_part(
                        SpaceTimeAtom::new(lhs.atom_end(), lhs.time_end()),
                        SpaceTimeAtom::new(
                            rhs.atom_start(),
                            rhs.time_start()
                                .unwrap_or(&(relative_time + self.compute_step)),
                        ),
                        vtime,
                    )
                }
            }
        };

        Ok(interpolated)
    }

    /// Compute step relative to 1 second
    pub fn relative_compute_step(&self) -> f32 {
        const ONE_SECOND: f32 = 1000.0;

        self.compute_step.num_milliseconds() as f32 / ONE_SECOND
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
        self.time_start + self.compute_step * (self.nodes.len() - 1) as i32
    }

    pub fn node_start(&self) -> Shared<TrackNode> {
        self.nodes.first().unwrap().share()
    }

    pub fn node_end(&self) -> Shared<TrackNode> {
        self.nodes.last().unwrap().share()
    }

    pub fn push_back(&mut self, node: TrackNode) {
        if self.nodes.push_back(node.into()) {
            self.time_start = self.time_start + self.compute_step;
        }
    }

    pub fn push_front(&mut self, node: TrackNode) {
        self.nodes.push_front(node.into());
        self.time_start = self.time_start - self.compute_step;
    }

    pub fn append<I: Iterator<Item = TrackNode>>(&mut self, iter: I) {
        let delta = self.nodes.append(iter.map(|node| node.into()));
        self.time_start = self.time_start + self.compute_step * delta;
    }

    pub fn prepend<I: Iterator<Item = TrackNode>>(&mut self, iter: I) {
        let added = self.nodes.prepend(iter.map(|node| node.into()));
        self.time_start = self.time_start - self.compute_step * added;
    }

    pub fn get_node(&mut self, vtime: &chrono::Duration) -> Option<Shared<TrackNode>> {
        if self.computed_range().contains(vtime) {
            let node_index = self.track_node_index(vtime);
            Some(self.nodes[node_index].share())
        } else {
            None
        }
    }

    pub fn is_fully_computed(&self) -> bool {
        self.nodes.len() == self.nodes.capacity()
    }

    fn truncate(&mut self, range: impl Into<TruncateRange<chrono::Duration>>) {
        let range = range.into();

        let begin_node_delta = self.nodes.truncate(range.map(|time| {
            // FIXME
            assert!(time.num_milliseconds() % self.compute_step.num_milliseconds() == 0);

            self.track_node_index(time)
        }));

        self.time_start = self.time_start + self.compute_step * begin_node_delta as i32;
    }

    fn track_node_index(&self, vtime: &chrono::Duration) -> usize {
        assert!(self.computed_range().contains(vtime));

        (self.track_relative_time(vtime).num_milliseconds() / self.compute_step.num_milliseconds())
            as usize
    }

    fn track_relative_time(&self, vtime: &chrono::Duration) -> chrono::Duration {
        *vtime - self.time_start
    }
}

fn interpolate_track_part(
    lhs: SpaceTimeAtom,
    rhs: SpaceTimeAtom,
    vtime: &chrono::Duration,
) -> Vector {
    const ONE_SECOND: f32 = 1000.0;

    hermite_interpolation(
        &lhs.track_atom.location,
        &lhs.track_atom.velocity,
        lhs.time.num_milliseconds() as f32 / ONE_SECOND,
        
        &rhs.track_atom.location,
        &rhs.track_atom.velocity,
        rhs.time.num_milliseconds() as f32 / ONE_SECOND,
        vtime.num_milliseconds() as f32 / ONE_SECOND
    )
}