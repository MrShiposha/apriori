use crate::{
    r#type::{
        ObjectId,
        ObjectName,
        Mass,
        Distance,
        Color, 
    },
    scene::track::Track,
};

pub struct Object4d {
    id: ObjectId,
    track: Track,
    name: ObjectName,
    mass: Mass,
    radius: Distance,
    color: Color,
    is_currently_computing: bool,
}

impl Object4d {
    pub fn new(
        track_size: usize,
        compute_step: chrono::Duration,
        name: ObjectName,
        mass: Mass,
        radius: Distance,
        color: Color,
    ) -> Self {
        Self {
            id: 0,
            track: Track::new(track_size, compute_step),
            name,
            mass,
            radius,
            color,
            is_currently_computing: false,
        }
    }

    pub fn id(&self) -> ObjectId {
        self.id
    }

    pub fn set_id(&mut self, id: ObjectId) {
        self.id = id;
    }

    pub fn name(&self) -> &ObjectName {
        &self.name
    }

    pub fn rename(&mut self, new_name: ObjectName) {
        self.name = new_name;
    }

    pub fn mass(&self) -> Mass {
        self.mass
    }

    pub fn radius(&self) -> Distance {
        self.radius
    }

    pub fn color(&self) -> &Color {
        &self.color
    }

    pub fn track(&self) -> &Track {
        &self.track
    }

    pub fn track_mut(&mut self) -> &mut Track {
        &mut self.track
    }

    pub fn set_computing(&mut self) {
        self.is_currently_computing = true;
    }

    pub fn reset_computing(&mut self) {
        self.is_currently_computing = false;
    }

    pub fn is_computing(&self) -> bool {
        self.is_currently_computing
    }
}
