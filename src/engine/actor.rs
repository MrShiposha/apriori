use {
    crate::object::{GenCoord, Object},
    std::sync::RwLock
};

pub struct Actor {
    object: Object,
    last_gen_coord: RwLock<Option<GenCoord>>,
    last_computed_time: chrono::Duration,
}

impl Actor {
    pub fn new(object: Object) -> Self {
        Self {
            object,
            last_gen_coord: RwLock::new(None),
            last_computed_time: chrono::Duration::zero(),
        }
    }

    pub fn object(&self) -> &Object {
        &self.object
    }

    pub fn last_gen_coord(&self) -> Option<GenCoord> {
        self.last_gen_coord.read().unwrap().clone()
    }

    pub fn set_last_gen_coord(&self, gen_coord: GenCoord) {
        *self.last_gen_coord.write().unwrap() = Some(gen_coord);
    }

    pub fn last_computed_time(&self) -> chrono::Duration {
        self.last_computed_time
    }

    pub fn set_last_computed_time(&mut self, time: chrono::Duration) {
        self.last_computed_time = time;
    }
}

impl Clone for Actor {
    fn clone(&self) -> Self {
        let object = self.object.clone();
        let last_computed_time = self.last_computed_time;

        Actor {
            object,
            last_gen_coord: RwLock::new(None),
            last_computed_time,
        }
    }
}
