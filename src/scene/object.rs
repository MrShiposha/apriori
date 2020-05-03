use crate::{
    r#type::{
        Color, 
        ObjectId,
        Distance,
        Mass
    },
    scene::track::Track,
};

pub struct Object4d {
    id: ObjectId,
    track: Track,
    mass: Mass,
    radius: Distance,
    color: Color,
    is_currently_computing: bool,
}

impl Object4d {
    pub fn new(
        id: ObjectId,
        track: Track,
        mass: Mass,
        radius: Distance,
        color: Color,
    ) -> Self {
        Self {
            id,
            track,
            mass,
            radius,
            color,
            is_currently_computing: false,
        }
    }

    pub fn id(&self) -> ObjectId {
        self.id
    }

    pub fn mass(&self) -> Mass {
        self.mass
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
}
