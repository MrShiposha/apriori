use {
    crate::{
        make_error,
        object::{GenCoord, Object},
        r#type::{LayerName, ObjectName, SessionId},
        Result,
    },
    std::collections::{hash_map::Entry, HashMap},
};

#[derive(Debug)]
pub struct Layer {
    name: LayerName,
    session_id: SessionId,
    objects_info: HashMap<Object, GenCoord>,
}

impl Layer {
    pub fn new(name: LayerName, session_id: SessionId) -> Self {
        Self {
            name,
            session_id,
            objects_info: HashMap::new(),
        }
    }

    pub fn name(&self) -> &LayerName {
        &self.name
    }

    pub fn add_object(&mut self, object: Object, coord: GenCoord) -> Result<()> {
        match self.objects_info.entry(object) {
            Entry::Vacant(entry) => {
                entry.insert(coord);
                Ok(())
            }
            Entry::Occupied(entry) => Err(make_error!(Error::Layer::ObjectAlreadyAdded(
                entry.key().name().clone()
            ))),
        }

        // match self.objects.get(object.name()) {
        //     Some(_) => Err(make_error!(Error::Layer::ObjectAlreadyAdded(object.name().clone()))),
        //     None => {
        //         self.objects.insert(object);
        //         Ok(())
        //     }
        // }
    }

    pub fn get_object(&self, object_name: &ObjectName) -> Option<(&Object, &GenCoord)> {
        self.objects_info.get_key_value(object_name)
    }

    pub fn iter_objects(&self) -> impl Iterator<Item = (&Object, &GenCoord)> {
        self.objects_info.iter()
    }

    pub fn take_objects(self) -> impl Iterator<Item = (Object, GenCoord)> {
        self.objects_info.into_iter()
    }
}
