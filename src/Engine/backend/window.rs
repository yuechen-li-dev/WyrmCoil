#![allow(non_snake_case)]

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WindowInputBackend;

impl WindowInputBackend {
    pub fn New() -> Self {
        Self
    }
}
