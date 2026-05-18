#![allow(non_snake_case)]

use std::collections::BTreeMap;

use crate::Engine::primitives::RenderSnapshot;
use crate::Engine::render::extract::BuildVisiblePrimitiveDemoBatch;
use crate::Engine::world::WorldGeometryRegistry;

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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldRayTriangleSource {
    pub TriangleId: i32,
    pub EntityId: crate::Engine::primitives::EntityId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorldGeometryRayScene {
    pub Scene: RayTriangleScene,
    pub Sources: Vec<WorldRayTriangleSource>,
}

pub fn BuildRayTriangleSceneFromWorldGeometryRegistry(
    registry: &WorldGeometryRegistry,
) -> WorldGeometryRayScene {
    let snapshot = registry.Snapshot();
    let mut triangles = Vec::with_capacity(snapshot.len());
    let mut sources = Vec::with_capacity(snapshot.len());
    for entry in snapshot {
        triangles.push(RayTriangle {
            Id: entry.TriangleId,
            A: entry.A,
            B: entry.B,
            C: entry.C,
        });
        sources.push(WorldRayTriangleSource {
            TriangleId: entry.TriangleId,
            EntityId: entry.EntityId,
        });
    }
    WorldGeometryRayScene {
        Scene: RayTriangleScene {
            Triangles: triangles,
        },
        Sources: sources,
    }
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VisiblePickResult {
    Hit(VisiblePickHit),
    Miss(VisiblePickMiss),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VisiblePickHit {
    pub QueryId: RayQueryId,
    pub EntityId: crate::Engine::primitives::EntityId,
    pub RenderItemIndex: usize,
    pub TriangleId: i32,
    pub TriangleIndexInItem: usize,
    pub Distance: f32,
    pub Position: RayVec3,
    pub Normal: RayVec3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VisiblePickMiss {
    pub QueryId: RayQueryId,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VisiblePickError {
    PickExecutionError(margaret::RayQueryExecutionError),
    MissingTriangleSource { TriangleId: i32 },
    UnexpectedCameraRayOutcome { QueryId: RayQueryId },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WorldPickResult {
    Hit(WorldPickHit),
    Miss(WorldPickMiss),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldPickHit {
    pub QueryId: RayQueryId,
    pub EntityId: crate::Engine::primitives::EntityId,
    pub TriangleId: i32,
    pub Distance: f32,
    pub Position: RayVec3,
    pub Normal: RayVec3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldPickMiss {
    pub QueryId: RayQueryId,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WorldPickError {
    RayQuery(margaret::RayQueryExecutionError),
    MissingTriangleSource { TriangleId: i32 },
    UnexpectedCameraRayOutcome { QueryId: RayQueryId },
}

pub fn ResolveWorldPickResult(
    outcome: RayQueryOutcome,
    triangle_sources: &[WorldRayTriangleSource],
) -> Result<WorldPickResult, WorldPickError> {
    match outcome {
        RayQueryOutcome::Hit(hit) => {
            let source = triangle_sources
                .iter()
                .find(|source| source.TriangleId == hit.TriangleId)
                .ok_or(WorldPickError::MissingTriangleSource {
                    TriangleId: hit.TriangleId,
                })?;
            Ok(WorldPickResult::Hit(WorldPickHit {
                QueryId: hit.QueryId,
                EntityId: source.EntityId,
                TriangleId: hit.TriangleId,
                Distance: hit.Distance,
                Position: hit.Position,
                Normal: hit.Normal,
            }))
        }
        RayQueryOutcome::Miss(miss) => Ok(WorldPickResult::Miss(WorldPickMiss {
            QueryId: miss.QueryId,
        })),
        RayQueryOutcome::CameraRay(camera_result) => {
            Err(WorldPickError::UnexpectedCameraRayOutcome {
                QueryId: camera_result.QueryId,
            })
        }
    }
}

pub fn PickWorldGeometryRegistry(
    registry: &WorldGeometryRegistry,
    camera_adapter: &margaret::MargaretCameraRayAdapter,
    screen_x: f32,
    screen_y: f32,
    query_id: RayQueryId,
) -> Result<WorldPickResult, WorldPickError> {
    let scene = BuildRayTriangleSceneFromWorldGeometryRegistry(registry);
    let mut store = RayQueryStore::New();
    let outcome = margaret::ExecutePickTriangleQuery(
        PickRayQueryRequest {
            QueryId: query_id,
            ScreenX: screen_x,
            ScreenY: screen_y,
            Scene: scene.Scene,
        },
        camera_adapter,
        &mut store,
    )
    .map_err(WorldPickError::RayQuery)?;

    ResolveWorldPickResult(outcome, &scene.Sources)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WorldBlackboardPickError {
    MissingCamera,
    MissingCursor,
    InvalidCursor { ScreenX: f32, ScreenY: f32 },
    Pick(WorldPickError),
}

pub fn PickWorldBlackboard(
    blackboard: &crate::Engine::world::WorldBlackboard,
    query_id: RayQueryId,
) -> Result<WorldPickResult, WorldBlackboardPickError> {
    let camera = blackboard
        .Camera
        .ok_or(WorldBlackboardPickError::MissingCamera)?;

    let (screen_x, screen_y) = blackboard
        .Input
        .CursorScreen()
        .ok_or(WorldBlackboardPickError::MissingCursor)?;

    if !screen_x.is_finite()
        || !screen_y.is_finite()
        || !(0.0..=1.0).contains(&screen_x)
        || !(0.0..=1.0).contains(&screen_y)
    {
        return Err(WorldBlackboardPickError::InvalidCursor {
            ScreenX: screen_x,
            ScreenY: screen_y,
        });
    }

    let adapter = camera.BuildMargaretCameraRayAdapter();
    PickWorldGeometryRegistry(&blackboard.Geometry, &adapter, screen_x, screen_y, query_id)
        .map_err(WorldBlackboardPickError::Pick)
}
pub fn ResolveVisiblePickResult(
    outcome: RayQueryOutcome,
    triangle_sources: &[RayTriangleSource],
) -> Result<VisiblePickResult, VisiblePickError> {
    match outcome {
        RayQueryOutcome::Hit(hit) => {
            let source = triangle_sources
                .iter()
                .find(|source| source.TriangleId == hit.TriangleId)
                .ok_or(VisiblePickError::MissingTriangleSource {
                    TriangleId: hit.TriangleId,
                })?;
            Ok(VisiblePickResult::Hit(VisiblePickHit {
                QueryId: hit.QueryId,
                EntityId: crate::Engine::primitives::EntityId(source.EntityId),
                RenderItemIndex: source.RenderItemIndex,
                TriangleId: hit.TriangleId,
                TriangleIndexInItem: source.TriangleIndexInItem,
                Distance: hit.Distance,
                Position: hit.Position,
                Normal: hit.Normal,
            }))
        }
        RayQueryOutcome::Miss(miss) => Ok(VisiblePickResult::Miss(VisiblePickMiss {
            QueryId: miss.QueryId,
        })),
        RayQueryOutcome::CameraRay(camera_result) => {
            Err(VisiblePickError::UnexpectedCameraRayOutcome {
                QueryId: camera_result.QueryId,
            })
        }
    }
}

pub fn PickVisibleRenderSnapshot(
    snapshot: &RenderSnapshot,
    camera_adapter: &margaret::MargaretCameraRayAdapter,
    screen_x: f32,
    screen_y: f32,
    query_id: RayQueryId,
    options: RenderSnapshotRaySceneOptions,
) -> Result<VisiblePickResult, VisiblePickError> {
    let scene = BuildVisiblePrimitiveRaySceneFromRenderSnapshot(snapshot, options);
    let mut store = RayQueryStore::New();
    let outcome = margaret::ExecutePickTriangleQuery(
        PickRayQueryRequest {
            QueryId: query_id,
            ScreenX: screen_x,
            ScreenY: screen_y,
            Scene: scene.Scene,
        },
        camera_adapter,
        &mut store,
    )
    .map_err(VisiblePickError::PickExecutionError)?;

    ResolveVisiblePickResult(outcome, &scene.TriangleSources)
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
