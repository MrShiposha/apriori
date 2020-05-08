use {
    crate::{
        shared::Shared,
        scene::Object4d,
        r#type::{
            ObjectId,
            TimeDirection,
        }
    }
};

pub struct Task {
    pub time_direction: TimeDirection,
    pub objects: Vec<(ObjectId, Shared<Object4d>)>,
}

impl Task {
    pub fn new(time_direction: TimeDirection, objects: Vec<(ObjectId, Shared<Object4d>)>) -> Self {
        Self {
            time_direction,
            objects
        }
    }
}