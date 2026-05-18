#![allow(non_snake_case)]

use std::collections::BTreeMap;

use crate::Engine::primitives::EntityId;
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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldBlackboard {
    pub Geometry: WorldGeometryRegistry,
    pub RayRequests: RayQueryRequestStore,
    pub RayResults: RayQueryStore,
}

impl WorldBlackboard {
    pub fn New() -> Self {
        Self::default()
    }

    pub fn Clear(&mut self) {
        self.Geometry.Clear();
        self.RayRequests = RayQueryRequestStore::New();
        self.RayResults = RayQueryStore::New();
    }
}
