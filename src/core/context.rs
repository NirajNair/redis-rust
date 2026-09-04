use crate::core::{aof::Aof, store::Store};

pub struct Context<'a> {
    pub store: &'a mut Store,
    pub aof: &'a mut Aof,
}

impl<'a> Context<'a> {
    pub fn new(store: &'a mut Store, aof: &'a mut Aof) -> Self {
        Context { store, aof }
    }
}
