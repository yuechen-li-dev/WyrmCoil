#![allow(non_snake_case)]

pub trait DwPhase: Copy {
    fn ToPc(self) -> u32;
    fn FromPc(pc: u32) -> Option<Self>;
}
