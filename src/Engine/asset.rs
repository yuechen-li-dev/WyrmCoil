#![allow(non_snake_case)]

use std::collections::BTreeMap;

use crate::Dunewyrm::SelectHighestUtilityTargetWithReport;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AssetRequestId(pub u32);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadBytesAssetRequest {
    pub RequestId: AssetRequestId,
    pub Path: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssetRequest {
    LoadBytes(LoadBytesAssetRequest),
}

impl AssetRequest {
    pub fn RequestId(&self) -> AssetRequestId {
        match self {
            AssetRequest::LoadBytes(request) => request.RequestId,
        }
    }

    pub fn Path(&self) -> &str {
        match self {
            AssetRequest::LoadBytes(request) => request.Path.as_str(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BytesLoadedAssetResult {
    pub RequestId: AssetRequestId,
    pub Path: String,
    pub Bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetLoadFailureKind {
    NotFound,
    IoError,
    InvalidRequest,
    NoFeasibleExecution,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetLoadFailure {
    pub RequestId: AssetRequestId,
    pub Path: String,
    pub Kind: AssetLoadFailureKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssetResult {
    BytesLoaded(BytesLoadedAssetResult),
    LoadFailed(AssetLoadFailure),
}

impl AssetResult {
    pub fn RequestId(&self) -> AssetRequestId {
        match self {
            AssetResult::BytesLoaded(result) => result.RequestId,
            AssetResult::LoadFailed(result) => result.RequestId,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AssetRequestStore {
    NextRequestId: u32,
    Requests: BTreeMap<AssetRequestId, AssetRequest>,
}

impl AssetRequestStore {
    pub fn New() -> Self {
        Self::default()
    }
    pub fn AllocateId(&mut self) -> AssetRequestId {
        let id = AssetRequestId(self.NextRequestId);
        self.NextRequestId = self.NextRequestId.saturating_add(1);
        id
    }
    pub fn Insert(&mut self, request: AssetRequest) -> Option<AssetRequest> {
        self.Requests.insert(request.RequestId(), request)
    }
    pub fn Get(&self, request_id: AssetRequestId) -> Option<&AssetRequest> {
        self.Requests.get(&request_id)
    }
    pub fn Take(&mut self, request_id: AssetRequestId) -> Option<AssetRequest> {
        self.Requests.remove(&request_id)
    }
    pub fn Remove(&mut self, request_id: AssetRequestId) -> Option<AssetRequest> {
        self.Requests.remove(&request_id)
    }
    pub fn Contains(&self, request_id: AssetRequestId) -> bool {
        self.Requests.contains_key(&request_id)
    }
    pub fn Snapshot(&self) -> Vec<AssetRequest> {
        self.Requests.values().cloned().collect()
    }
    pub fn Clear(&mut self) {
        self.Requests.clear();
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AssetResultStore {
    Completed: BTreeMap<AssetRequestId, AssetResult>,
}

impl AssetResultStore {
    pub fn New() -> Self {
        Self::default()
    }
    pub fn Store(&mut self, result: AssetResult) -> Option<AssetResult> {
        self.Completed.insert(result.RequestId(), result)
    }
    pub fn Get(&self, request_id: AssetRequestId) -> Option<&AssetResult> {
        self.Completed.get(&request_id)
    }
    pub fn Take(&mut self, request_id: AssetRequestId) -> Option<AssetResult> {
        self.Completed.remove(&request_id)
    }
    pub fn Contains(&self, request_id: AssetRequestId) -> bool {
        self.Completed.contains_key(&request_id)
    }
    pub fn Snapshot(&self) -> Vec<AssetResult> {
        self.Completed.values().cloned().collect()
    }
    pub fn Clear(&mut self) {
        self.Completed.clear();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetExecutionMode {
    ImmediateBytesLoad,
    DeferredUnsupported,
    NoAssetExecutionFeasible,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AssetExecutionConstraints {
    pub AllowImmediate: bool,
    pub AllowDeferred: bool,
}
impl Default for AssetExecutionConstraints {
    fn default() -> Self {
        Self {
            AllowImmediate: true,
            AllowDeferred: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AssetExecutionPlan {
    pub Mode: AssetExecutionMode,
    pub Reason: &'static str,
    pub Utility: crate::Dunewyrm::DwUtilityDecisionReport<AssetExecutionMode>,
}

pub fn PlanAssetRequestExecution(
    request: &AssetRequest,
    constraints: AssetExecutionConstraints,
) -> AssetExecutionPlan {
    let path_valid = !request.Path().trim().is_empty();
    let immediate_score = if constraints.AllowImmediate && path_valid {
        1.0
    } else {
        0.0
    };
    let deferred_score = if constraints.AllowDeferred { 0.2 } else { 0.0 };
    let scored = [
        (AssetExecutionMode::ImmediateBytesLoad, immediate_score),
        (AssetExecutionMode::DeferredUnsupported, deferred_score),
        (AssetExecutionMode::NoAssetExecutionFeasible, 0.01),
    ];
    let utility = SelectHighestUtilityTargetWithReport(&scored);
    let selected = utility
        .Selected
        .unwrap_or(AssetExecutionMode::NoAssetExecutionFeasible);

    let (mode, reason) = if !path_valid {
        (
            AssetExecutionMode::NoAssetExecutionFeasible,
            "invalid request path",
        )
    } else {
        match selected {
            AssetExecutionMode::ImmediateBytesLoad => (
                AssetExecutionMode::ImmediateBytesLoad,
                "immediate bytes load allowed",
            ),
            AssetExecutionMode::DeferredUnsupported => {
                if constraints.AllowImmediate {
                    (
                        AssetExecutionMode::ImmediateBytesLoad,
                        "deferred unsupported in M83; immediate selected",
                    )
                } else {
                    (
                        AssetExecutionMode::NoAssetExecutionFeasible,
                        "deferred unsupported in M83",
                    )
                }
            }
            AssetExecutionMode::NoAssetExecutionFeasible => (
                AssetExecutionMode::NoAssetExecutionFeasible,
                "no feasible asset execution mode",
            ),
        }
    };

    AssetExecutionPlan {
        Mode: mode,
        Reason: reason,
        Utility: utility,
    }
}

pub fn ExecuteAssetRequest(
    request: AssetRequest,
    constraints: AssetExecutionConstraints,
) -> AssetResult {
    let plan = PlanAssetRequestExecution(&request, constraints);
    let request_id = request.RequestId();
    let path = request.Path().to_string();
    let path_is_empty = path.trim().is_empty();
    if plan.Mode != AssetExecutionMode::ImmediateBytesLoad {
        return AssetResult::LoadFailed(AssetLoadFailure {
            RequestId: request_id,
            Path: path,
            Kind: if path_is_empty {
                AssetLoadFailureKind::InvalidRequest
            } else {
                AssetLoadFailureKind::NoFeasibleExecution
            },
        });
    }

    match std::fs::read(&path) {
        Ok(bytes) => AssetResult::BytesLoaded(BytesLoadedAssetResult {
            RequestId: request_id,
            Path: path,
            Bytes: bytes,
        }),
        Err(error) => {
            let kind = if error.kind() == std::io::ErrorKind::NotFound {
                AssetLoadFailureKind::NotFound
            } else {
                AssetLoadFailureKind::IoError
            };
            AssetResult::LoadFailed(AssetLoadFailure {
                RequestId: request_id,
                Path: path,
                Kind: kind,
            })
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetExecutionError {
    MissingRequest,
    RequestIdOutOfRange { RequestId: AssetRequestId },
}

pub fn ExecuteAssetRequestById(
    request_id: AssetRequestId,
    blackboard: &mut crate::Engine::world::WorldBlackboard,
    mailbox: &mut crate::DwMailbox,
    completion_kind: u32,
    constraints: AssetExecutionConstraints,
) -> Result<(), AssetExecutionError> {
    let request = blackboard
        .Assets
        .Requests
        .Take(request_id)
        .ok_or(AssetExecutionError::MissingRequest)?;
    let result = ExecuteAssetRequest(request, constraints);
    blackboard.Assets.Results.Store(result);
    if request_id.0 > i32::MAX as u32 {
        return Err(AssetExecutionError::RequestIdOutOfRange {
            RequestId: request_id,
        });
    }
    mailbox.Enqueue(crate::DwMessage::I32(completion_kind, request_id.0 as i32));
    Ok(())
}
