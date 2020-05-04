use {
    crate::r#type::{
        AttractorId,
        Mass,
        GravityCoeff,
        Vector,
    }
};

pub struct Attractor {
    location: Vector,
    mass: Mass,
    gravity_coeff: GravityCoeff,
}

impl Attractor {
    pub fn new(location: Vector, mass: Mass, gravity_coeff: GravityCoeff) -> Self {
        Self {
            location,
            mass,
            gravity_coeff,
        }
    }

    pub fn location(&self) -> &Vector {
        &self.location
    }

    pub fn mass(&self) -> Mass {
        self.mass
    }

    pub fn gravity_coeff(&self) -> GravityCoeff {
        self.gravity_coeff
    }
}