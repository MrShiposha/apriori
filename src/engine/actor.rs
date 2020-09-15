use crate::object::{GenCoord, Object};

pub struct Actor {
    object: Object,
    last_location: Option<GenCoord>,
}

impl Actor {
    pub fn new(object: Object) -> Self {
        Self {
            object,
            last_location: None,
        }
    }

    pub fn object(&self) -> &Object {
        &self.object
    }

    pub fn set_last_location(&mut self, last_location: GenCoord) {
        self.last_location = Some(last_location);
    }

    pub fn last_gen_coord(&self) -> Option<GenCoord> {
        self.last_location.clone()
    }
}

impl Clone for Actor {
    fn clone(&self) -> Self {
        let object = self.object.clone();

        Actor {
            object,
            last_location: None,
        }
    }
}
