#![allow(non_snake_case)]

use std::collections::BTreeMap;

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
pub struct TriangleRayQueryRequest {
    pub QueryId: RayQueryId,
    pub Ray: Ray3,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn SampleResult(query_id: u32) -> CameraRayResult {
        CameraRayResult {
            QueryId: RayQueryId(query_id),
            Origin: RayVec3 {
                X: query_id as f32,
                Y: 1.0,
                Z: 2.0,
            },
            Direction: RayVec3 {
                X: 0.0,
                Y: 0.0,
                Z: -1.0,
            },
        }
    }

    #[test]
    fn QueryStoreStoresAndRetrievesById() {
        let mut store = RayQueryStore::New();
        store.StoreCompleted(SampleResult(3));

        assert_eq!(store.GetCompleted(RayQueryId(3)), Some(SampleResult(3)));
        assert_eq!(store.GetCompleted(RayQueryId(99)), None);
    }

    #[test]
    fn QueryStoreSnapshotIsDeterministicByQueryId() {
        let mut store = RayQueryStore::New();
        store.StoreCompleted(SampleResult(7));
        store.StoreCompleted(SampleResult(2));
        store.StoreCompleted(SampleResult(5));

        let snapshot = store.CompletedSnapshot();
        assert_eq!(snapshot.len(), 3);
        assert_eq!(snapshot[0].QueryId, RayQueryId(2));
        assert_eq!(snapshot[1].QueryId, RayQueryId(5));
        assert_eq!(snapshot[2].QueryId, RayQueryId(7));
    }

    #[test]
    fn QueryStoreAllocatesDeterministicIds() {
        let mut store = RayQueryStore::New();
        assert_eq!(store.AllocateQueryId(), RayQueryId(0));
        assert_eq!(store.AllocateQueryId(), RayQueryId(1));
    }
}
