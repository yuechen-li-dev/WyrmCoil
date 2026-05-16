#![allow(non_snake_case)]

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct DwFrameId {
    pub Domain: u64,
    pub Local: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct DwActId {
    pub Domain: u64,
    pub Local: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DwActRequest {
    pub Id: DwActId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DwDeferredAct {
    pub Request: DwActRequest,
    pub DueTick: u64,
}
