#![allow(non_snake_case)]

use std::collections::BTreeMap;

use crate::Engine::primitives::RenderSnapshot;
use crate::Engine::render::extract::BuildVisiblePrimitiveDemoBatch;

pub mod margaret;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct RayQueryId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayVec3 {
    pub X: f32,
    pub Y: f32,
    pub Z: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraRayRequest {
    pub QueryId: RayQueryId,
    pub ScreenX: f32,
    pub ScreenY: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraRayResult {
    pub QueryId: RayQueryId,
    pub Origin: RayVec3,
    pub Direction: RayVec3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray3 {
    pub Origin: RayVec3,
    pub Direction: RayVec3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayTriangle {
    pub Id: i32,
    pub A: RayVec3,
    pub B: RayVec3,
    pub C: RayVec3,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RayTriangleScene {
    pub Triangles: Vec<RayTriangle>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderSnapshotRaySceneOptions {
    pub PlaneZ: f32,
}

impl Default for RenderSnapshotRaySceneOptions {
    fn default() -> Self {
        Self { PlaneZ: -1.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayTriangleSource {
    pub TriangleId: i32,
    pub RenderItemIndex: usize,
    pub EntityId: usize,
    pub TriangleIndexInItem: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderSnapshotRayScene {
    pub Scene: RayTriangleScene,
    pub TriangleSources: Vec<RayTriangleSource>,
}

pub fn BuildVisiblePrimitiveRaySceneFromRenderSnapshot(
    snapshot: &RenderSnapshot,
    options: RenderSnapshotRaySceneOptions,
) -> RenderSnapshotRayScene {
    let batch = BuildVisiblePrimitiveDemoBatch(snapshot);
    let mut triangles = Vec::with_capacity(batch.Vertices.len() / 3);
    let mut sources = Vec::with_capacity(batch.Vertices.len() / 3);

    for (item_index, chunk) in batch.Vertices.chunks_exact(6).enumerate() {
        let entity_id = snapshot.Items[item_index].Entity.0;
        for triangle_index_in_item in 0..2usize {
            let base = triangle_index_in_item * 3;
            let triangle_id = (item_index as i32) * 2 + triangle_index_in_item as i32;
            triangles.push(RayTriangle {
                Id: triangle_id,
                A: RayVec3 {
                    X: chunk[base].X,
                    Y: chunk[base].Y,
                    Z: options.PlaneZ,
                },
                B: RayVec3 {
                    X: chunk[base + 1].X,
                    Y: chunk[base + 1].Y,
                    Z: options.PlaneZ,
                },
                C: RayVec3 {
                    X: chunk[base + 2].X,
                    Y: chunk[base + 2].Y,
                    Z: options.PlaneZ,
                },
            });
            sources.push(RayTriangleSource {
                TriangleId: triangle_id,
                RenderItemIndex: item_index,
                EntityId: entity_id,
                TriangleIndexInItem: triangle_index_in_item,
            });
        }
    }

    RenderSnapshotRayScene {
        Scene: RayTriangleScene {
            Triangles: triangles,
        },
        TriangleSources: sources,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TriangleRayQueryRequest {
    pub QueryId: RayQueryId,
    pub Ray: Ray3,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PickRayQueryRequest {
    pub QueryId: RayQueryId,
    pub ScreenX: f32,
    pub ScreenY: f32,
    pub Scene: RayTriangleScene,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RayQueryRequest {
    CameraRay(CameraRayRequest),
    TriangleRay {
        Request: TriangleRayQueryRequest,
        Scene: RayTriangleScene,
    },
    PickTriangle(PickRayQueryRequest),
}

impl RayQueryRequest {
    pub fn QueryId(&self) -> RayQueryId {
        match self {
            RayQueryRequest::CameraRay(request) => request.QueryId,
            RayQueryRequest::TriangleRay { Request, .. } => Request.QueryId,
            RayQueryRequest::PickTriangle(request) => request.QueryId,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RayQueryRequestStore {
    NextQueryId: u32,
    Requests: BTreeMap<RayQueryId, RayQueryRequest>,
}

impl RayQueryRequestStore {
    pub fn New() -> Self {
        Self::default()
    }

    pub fn AllocateQueryId(&mut self) -> RayQueryId {
        let query_id = RayQueryId(self.NextQueryId);
        self.NextQueryId = self.NextQueryId.saturating_add(1);
        query_id
    }

    pub fn Insert(&mut self, request: RayQueryRequest) -> Option<RayQueryRequest> {
        self.Requests.insert(request.QueryId(), request)
    }

    pub fn Get(&self, query_id: RayQueryId) -> Option<&RayQueryRequest> {
        self.Requests.get(&query_id)
    }

    pub fn Take(&mut self, query_id: RayQueryId) -> Option<RayQueryRequest> {
        self.Requests.remove(&query_id)
    }

    pub fn Remove(&mut self, query_id: RayQueryId) -> Option<RayQueryRequest> {
        self.Requests.remove(&query_id)
    }

    pub fn Contains(&self, query_id: RayQueryId) -> bool {
        self.Requests.contains_key(&query_id)
    }

    pub fn Snapshot(&self) -> Vec<RayQueryRequest> {
        self.Requests.values().cloned().collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHitResult {
    pub QueryId: RayQueryId,
    pub TriangleId: i32,
    pub Distance: f32,
    pub Position: RayVec3,
    pub Normal: RayVec3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayMissResult {
    pub QueryId: RayQueryId,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RayQueryOutcome {
    CameraRay(CameraRayResult),
    Hit(RayHitResult),
    Miss(RayMissResult),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RayQueryStore {
    NextQueryId: u32,
    Completed: BTreeMap<RayQueryId, RayQueryOutcome>,
}

impl RayQueryStore {
    /* unchanged methods */
    pub fn New() -> Self {
        Self::default()
    }
    pub fn AllocateQueryId(&mut self) -> RayQueryId {
        let query_id = RayQueryId(self.NextQueryId);
        self.NextQueryId = self.NextQueryId.saturating_add(1);
        query_id
    }
    pub fn StoreCompleted(&mut self, result: CameraRayResult) {
        self.Completed
            .insert(result.QueryId, RayQueryOutcome::CameraRay(result));
    }
    pub fn StoreHitResult(&mut self, result: RayHitResult) {
        self.Completed
            .insert(result.QueryId, RayQueryOutcome::Hit(result));
    }
    pub fn StoreMissResult(&mut self, result: RayMissResult) {
        self.Completed
            .insert(result.QueryId, RayQueryOutcome::Miss(result));
    }
    pub fn GetOutcome(&self, query_id: RayQueryId) -> Option<RayQueryOutcome> {
        self.Completed.get(&query_id).copied()
    }
    pub fn GetCompleted(&self, query_id: RayQueryId) -> Option<CameraRayResult> {
        match self.Completed.get(&query_id).copied() {
            Some(RayQueryOutcome::CameraRay(result)) => Some(result),
            _ => None,
        }
    }
    pub fn GetHitResult(&self, query_id: RayQueryId) -> Option<RayHitResult> {
        match self.Completed.get(&query_id).copied() {
            Some(RayQueryOutcome::Hit(result)) => Some(result),
            _ => None,
        }
    }
    pub fn CompletedSnapshot(&self) -> Vec<CameraRayResult> {
        self.Completed
            .values()
            .filter_map(|outcome| match outcome {
                RayQueryOutcome::CameraRay(result) => Some(*result),
                _ => None,
            })
            .collect()
    }
    pub fn OutcomeSnapshot(&self) -> Vec<RayQueryOutcome> {
        self.Completed.values().copied().collect()
    }
}
