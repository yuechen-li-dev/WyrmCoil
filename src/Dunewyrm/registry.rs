#![allow(non_snake_case)]

use std::collections::HashMap;

use crate::DwControl;
use crate::ids::DwFrameId;
use crate::session::DwFrameCtx;

pub type DwFrameFn = for<'a> fn(&mut DwFrameCtx<'a>) -> DwControl;

#[derive(Clone, Copy)]
pub struct DwFrameDef {
    pub Id: DwFrameId,
    pub Step: DwFrameFn,
    pub DebugName: &'static str,
}

pub struct DwFrameRegistry {
    Frames: HashMap<DwFrameId, DwFrameDef>,
}

impl DwFrameRegistry {
    pub fn New() -> Self {
        Self {
            Frames: HashMap::new(),
        }
    }

    pub fn Register(&mut self, frame: DwFrameDef) -> Result<(), &'static str> {
        if self.Frames.contains_key(&frame.Id) {
            return Err("duplicate frame id");
        }

        self.Frames.insert(frame.Id, frame);
        Ok(())
    }

    pub fn Find(&self, id: DwFrameId) -> Option<&DwFrameDef> {
        self.Frames.get(&id)
    }
}
