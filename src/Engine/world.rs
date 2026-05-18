#![allow(non_snake_case)]

use std::collections::BTreeMap;

use margaret_core::camera::Camera;
use margaret_core::math::{Point3, Vec3};

use crate::Engine::primitives::EntityId;
use crate::Engine::ray::margaret::MargaretCameraRayAdapter;
use crate::Engine::ray::{RayQueryRequestStore, RayQueryStore, RayVec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PickableTriangle {
    pub EntityId: EntityId,
    pub TriangleId: i32,
    pub A: RayVec3,
    pub B: RayVec3,
    pub C: RayVec3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RegisterTriangleError {
    DuplicateTriangleId { TriangleId: i32 },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldGeometryRegistry {
    Triangles: BTreeMap<i32, PickableTriangle>,
}

impl WorldGeometryRegistry {
    pub fn New() -> Self {
        Self::default()
    }

    pub fn Clear(&mut self) {
        self.Triangles.clear();
    }

    pub fn RegisterTriangle(
        &mut self,
        triangle: PickableTriangle,
    ) -> Result<(), RegisterTriangleError> {
        if self.Triangles.contains_key(&triangle.TriangleId) {
            return Err(RegisterTriangleError::DuplicateTriangleId {
                TriangleId: triangle.TriangleId,
            });
        }

        self.Triangles.insert(triangle.TriangleId, triangle);
        Ok(())
    }

    pub fn GetTriangle(&self, triangle_id: i32) -> Option<PickableTriangle> {
        self.Triangles.get(&triangle_id).copied()
    }

    pub fn Triangles(&self) -> Vec<PickableTriangle> {
        self.Snapshot()
    }

    pub fn Snapshot(&self) -> Vec<PickableTriangle> {
        self.Triangles.values().copied().collect()
    }

    pub fn Len(&self) -> usize {
        self.Triangles.len()
    }

    pub fn IsEmpty(&self) -> bool {
        self.Triangles.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldCameraResource {
    pub Position: RayVec3,
    pub Forward: RayVec3,
    pub Up: RayVec3,
    pub VerticalFovDegrees: f32,
    pub Width: u32,
    pub Height: u32,
}

impl WorldCameraResource {
    pub fn BuildMargaretCameraRayAdapter(&self) -> MargaretCameraRayAdapter {
        MargaretCameraRayAdapter {
            Camera: Camera::New(
                "world-blackboard-picker-camera",
                Point3::New(self.Position.X, self.Position.Y, self.Position.Z),
                Vec3::New(self.Forward.X, self.Forward.Y, self.Forward.Z),
                Vec3::New(self.Up.X, self.Up.Y, self.Up.Z),
                self.VerticalFovDegrees,
            ),
            Width: self.Width,
            Height: self.Height,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldInputResource {
    pub CursorScreenX: Option<f32>,
    pub CursorScreenY: Option<f32>,
}

impl Default for WorldInputResource {
    fn default() -> Self {
        Self {
            CursorScreenX: None,
            CursorScreenY: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WorldInputError {
    InvalidCursor {
        CursorScreenX: f32,
        CursorScreenY: f32,
    },
}

impl WorldInputResource {
    pub fn New() -> Self {
        Self::default()
    }

    pub fn SetCursorScreen(&mut self, screen_x: f32, screen_y: f32) -> Result<(), WorldInputError> {
        if !screen_x.is_finite()
            || !screen_y.is_finite()
            || !(0.0..=1.0).contains(&screen_x)
            || !(0.0..=1.0).contains(&screen_y)
        {
            return Err(WorldInputError::InvalidCursor {
                CursorScreenX: screen_x,
                CursorScreenY: screen_y,
            });
        }

        self.CursorScreenX = Some(screen_x);
        self.CursorScreenY = Some(screen_y);
        Ok(())
    }

    pub fn ClearCursor(&mut self) {
        self.CursorScreenX = None;
        self.CursorScreenY = None;
    }

    pub fn CursorScreen(&self) -> Option<(f32, f32)> {
        Some((self.CursorScreenX?, self.CursorScreenY?))
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldBlackboard {
    pub Geometry: WorldGeometryRegistry,
    pub Camera: Option<WorldCameraResource>,
    pub Input: WorldInputResource,
    pub RayRequests: RayQueryRequestStore,
    pub RayResults: RayQueryStore,
}

impl WorldBlackboard {
    pub fn New() -> Self {
        Self::default()
    }

    pub fn Clear(&mut self) {
        self.Geometry.Clear();
        self.Camera = None;
        self.Input = WorldInputResource::New();
        self.RayRequests = RayQueryRequestStore::New();
        self.RayResults = RayQueryStore::New();
    }
}
