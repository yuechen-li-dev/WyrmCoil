#![allow(non_snake_case)]

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub X: f32,
    pub Y: f32,
}
impl Vec2 {
    pub fn Zero() -> Self {
        Self { X: 0.0, Y: 0.0 }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EntityId(pub usize);

#[derive(Clone, Debug, PartialEq)]
pub struct RenderItem {
    pub Entity: EntityId,
    pub Position: Vec2,
    pub SpriteId: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderSnapshot {
    pub Frame: u64,
    pub Items: Vec<RenderItem>,
}
