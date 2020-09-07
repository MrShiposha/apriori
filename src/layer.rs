use crate::r#type::LayerName;

#[derive(Debug)]
pub struct Layer {
    name: LayerName,
}

impl Layer {
    pub fn new(name: LayerName) -> Self {
        Self {
            name
        }
    }

    pub fn name(&self) -> &LayerName {
        &self.name
    }
}