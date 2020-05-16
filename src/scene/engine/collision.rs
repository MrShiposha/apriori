use {
    crate::{
        r#type::{
            ObjectId,
            RelativeTime,
            Vector,
        }
    }
};

pub struct CollisionDescriptor {
    pub object_location: Vector,
    pub colliding_object_location: Vector,
    pub colliding_object_id: ObjectId,
    pub collision_time: RelativeTime,
}