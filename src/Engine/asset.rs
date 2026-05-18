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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageDecodeFormat {
    Ppm,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodeImageAssetRequest {
    pub RequestId: AssetRequestId,
    pub SourceName: String,
    pub Bytes: Vec<u8>,
    pub Format: ImageDecodeFormat,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssetRequest {
    LoadBytes(LoadBytesAssetRequest),
    DecodeImage(DecodeImageAssetRequest),
}

impl AssetRequest {
    pub fn RequestId(&self) -> AssetRequestId {
        match self {
            AssetRequest::LoadBytes(request) => request.RequestId,
            AssetRequest::DecodeImage(request) => request.RequestId,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BytesLoadedAssetResult {
    pub RequestId: AssetRequestId,
    pub Path: String,
    pub Bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodedImageAsset {
    pub RequestId: AssetRequestId,
    pub SourceName: String,
    pub Width: u32,
    pub Height: u32,
    pub Rgba8: Vec<u8>,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageDecodeFailureKind {
    UnsupportedFormat,
    InvalidData,
    InvalidDimensions,
    NoFeasibleExecution,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageDecodeFailure {
    pub RequestId: AssetRequestId,
    pub SourceName: String,
    pub Kind: ImageDecodeFailureKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssetResult {
    BytesLoaded(BytesLoadedAssetResult),
    ImageDecoded(DecodedImageAsset),
    LoadFailed(AssetLoadFailure),
    DecodeFailed(ImageDecodeFailure),
}

impl AssetResult {
    pub fn RequestId(&self) -> AssetRequestId {
        match self {
            AssetResult::BytesLoaded(result) => result.RequestId,
            AssetResult::ImageDecoded(result) => result.RequestId,
            AssetResult::LoadFailed(result) => result.RequestId,
            AssetResult::DecodeFailed(result) => result.RequestId,
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
    ImmediateImageDecode,
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
    let request_valid = match request {
        AssetRequest::LoadBytes(request) => !request.Path.trim().is_empty(),
        AssetRequest::DecodeImage(request) => {
            !request.SourceName.trim().is_empty() && !request.Bytes.is_empty()
        }
    };
    let immediate_mode = match request {
        AssetRequest::LoadBytes(_) => AssetExecutionMode::ImmediateBytesLoad,
        AssetRequest::DecodeImage(_) => AssetExecutionMode::ImmediateImageDecode,
    };
    let immediate_score = if constraints.AllowImmediate && request_valid {
        1.0
    } else {
        0.0
    };
    let deferred_score = if constraints.AllowDeferred { 0.2 } else { 0.0 };
    let scored = [
        (immediate_mode, immediate_score),
        (AssetExecutionMode::DeferredUnsupported, deferred_score),
        (AssetExecutionMode::NoAssetExecutionFeasible, 0.01),
    ];
    let utility = SelectHighestUtilityTargetWithReport(&scored);
    let selected = utility
        .Selected
        .unwrap_or(AssetExecutionMode::NoAssetExecutionFeasible);
    let (mode, reason) = if !request_valid {
        (
            AssetExecutionMode::NoAssetExecutionFeasible,
            "invalid asset request",
        )
    } else {
        match selected {
            AssetExecutionMode::ImmediateBytesLoad => (
                AssetExecutionMode::ImmediateBytesLoad,
                "immediate bytes load allowed",
            ),
            AssetExecutionMode::ImmediateImageDecode => (
                AssetExecutionMode::ImmediateImageDecode,
                "immediate image decode allowed",
            ),
            AssetExecutionMode::DeferredUnsupported => {
                if constraints.AllowImmediate {
                    (
                        immediate_mode,
                        "deferred unsupported in M84; immediate selected",
                    )
                } else {
                    (
                        AssetExecutionMode::NoAssetExecutionFeasible,
                        "deferred unsupported in M84",
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

fn ParsePpmToken(bytes: &[u8], cursor: &mut usize) -> Option<String> {
    while *cursor < bytes.len() {
        let b = bytes[*cursor];
        if b == b'#' {
            while *cursor < bytes.len() && bytes[*cursor] != b'\n' {
                *cursor += 1;
            }
        } else if b.is_ascii_whitespace() {
            *cursor += 1;
        } else {
            break;
        }
    }
    if *cursor >= bytes.len() {
        return None;
    }
    let start = *cursor;
    while *cursor < bytes.len() && !bytes[*cursor].is_ascii_whitespace() {
        *cursor += 1;
    }
    Some(String::from_utf8_lossy(&bytes[start..*cursor]).to_string())
}

fn DecodePpmToRgba8(
    request: &DecodeImageAssetRequest,
) -> Result<DecodedImageAsset, ImageDecodeFailureKind> {
    let mut cursor = 0usize;
    let magic =
        ParsePpmToken(&request.Bytes, &mut cursor).ok_or(ImageDecodeFailureKind::InvalidData)?;
    if magic != "P6" {
        return Err(ImageDecodeFailureKind::InvalidData);
    }
    let width: u32 = ParsePpmToken(&request.Bytes, &mut cursor)
        .ok_or(ImageDecodeFailureKind::InvalidData)?
        .parse()
        .map_err(|_| ImageDecodeFailureKind::InvalidDimensions)?;
    let height: u32 = ParsePpmToken(&request.Bytes, &mut cursor)
        .ok_or(ImageDecodeFailureKind::InvalidData)?
        .parse()
        .map_err(|_| ImageDecodeFailureKind::InvalidDimensions)?;
    if width == 0 || height == 0 {
        return Err(ImageDecodeFailureKind::InvalidDimensions);
    }
    let max_value: u32 = ParsePpmToken(&request.Bytes, &mut cursor)
        .ok_or(ImageDecodeFailureKind::InvalidData)?
        .parse()
        .map_err(|_| ImageDecodeFailureKind::InvalidData)?;
    if max_value != 255 {
        return Err(ImageDecodeFailureKind::InvalidData);
    }
    while cursor < request.Bytes.len() && request.Bytes[cursor].is_ascii_whitespace() {
        cursor += 1;
    }
    let pixels = &request.Bytes[cursor..];
    let rgb_len = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(3);
    if pixels.len() != rgb_len {
        return Err(ImageDecodeFailureKind::InvalidData);
    }
    let mut rgba = Vec::with_capacity((width as usize) * (height as usize) * 4);
    for chunk in pixels.chunks_exact(3) {
        rgba.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
    }
    Ok(DecodedImageAsset {
        RequestId: request.RequestId,
        SourceName: request.SourceName.clone(),
        Width: width,
        Height: height,
        Rgba8: rgba,
    })
}

pub fn ExecuteAssetRequest(
    request: AssetRequest,
    constraints: AssetExecutionConstraints,
) -> AssetResult {
    let plan = PlanAssetRequestExecution(&request, constraints);
    match request {
        AssetRequest::LoadBytes(request) => {
            if plan.Mode != AssetExecutionMode::ImmediateBytesLoad {
                let path_is_empty = request.Path.trim().is_empty();
                return AssetResult::LoadFailed(AssetLoadFailure {
                    RequestId: request.RequestId,
                    Path: request.Path,
                    Kind: if path_is_empty {
                        AssetLoadFailureKind::InvalidRequest
                    } else {
                        AssetLoadFailureKind::NoFeasibleExecution
                    },
                });
            }
            match std::fs::read(&request.Path) {
                Ok(bytes) => AssetResult::BytesLoaded(BytesLoadedAssetResult {
                    RequestId: request.RequestId,
                    Path: request.Path,
                    Bytes: bytes,
                }),
                Err(error) => AssetResult::LoadFailed(AssetLoadFailure {
                    RequestId: request.RequestId,
                    Path: request.Path,
                    Kind: if error.kind() == std::io::ErrorKind::NotFound {
                        AssetLoadFailureKind::NotFound
                    } else {
                        AssetLoadFailureKind::IoError
                    },
                }),
            }
        }
        AssetRequest::DecodeImage(request) => {
            if plan.Mode != AssetExecutionMode::ImmediateImageDecode {
                let source_is_empty = request.SourceName.trim().is_empty();
                let bytes_empty = request.Bytes.is_empty();
                return AssetResult::DecodeFailed(ImageDecodeFailure {
                    RequestId: request.RequestId,
                    SourceName: request.SourceName,
                    Kind: if source_is_empty || bytes_empty {
                        ImageDecodeFailureKind::InvalidData
                    } else {
                        ImageDecodeFailureKind::NoFeasibleExecution
                    },
                });
            }
            match request.Format {
                ImageDecodeFormat::Ppm => match DecodePpmToRgba8(&request) {
                    Ok(result) => AssetResult::ImageDecoded(result),
                    Err(kind) => AssetResult::DecodeFailed(ImageDecodeFailure {
                        RequestId: request.RequestId,
                        SourceName: request.SourceName,
                        Kind: kind,
                    }),
                },
            }
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
