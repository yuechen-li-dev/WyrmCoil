#![allow(non_snake_case)]

use std::fs;

use wyrmcoil::Engine::{
    AssetExecutionConstraints, AssetExecutionError, AssetExecutionMode, AssetLoadFailureKind,
    AssetRequest, AssetRequestId, AssetRequestStore, AssetResult, AssetResultStore,
    ExecuteAssetRequest, ExecuteAssetRequestById, LoadBytesAssetRequest, PlanAssetRequestExecution,
    WorldBlackboard,
};
use wyrmcoil::{DwMailbox, DwMessage};

fn TempPath(name: &str) -> String {
    format!(
        "{}/{}_{}",
        std::env::temp_dir().display(),
        name,
        std::process::id()
    )
}

#[test]
fn AssetStoresAllocateAndSnapshotDeterministically() {
    let mut requests = AssetRequestStore::New();
    let id0 = requests.AllocateId();
    let id1 = requests.AllocateId();
    assert_eq!(id0, AssetRequestId(0), "first allocated id should be zero");
    assert_eq!(
        id1,
        AssetRequestId(1),
        "second allocated id should increment deterministically"
    );

    let replaced = requests.Insert(AssetRequest::LoadBytes(LoadBytesAssetRequest {
        RequestId: id0,
        Path: "a.bin".to_string(),
    }));
    assert!(
        replaced.is_none(),
        "first insert should not replace an existing request"
    );
    let replaced = requests.Insert(AssetRequest::LoadBytes(LoadBytesAssetRequest {
        RequestId: id0,
        Path: "b.bin".to_string(),
    }));
    assert!(
        replaced.is_some(),
        "duplicate id insert should replace existing request by policy"
    );
    assert_eq!(
        requests.Snapshot().len(),
        1,
        "snapshot should include deterministic single replaced request"
    );

    let mut results = AssetResultStore::New();
    results.Store(AssetResult::LoadFailed(
        wyrmcoil::Engine::AssetLoadFailure {
            RequestId: id1,
            Path: "x".into(),
            Kind: AssetLoadFailureKind::InvalidRequest,
        },
    ));
    assert!(
        results.Contains(id1),
        "result store should contain stored id"
    );
    results.Clear();
    assert!(
        results.Snapshot().is_empty(),
        "clear should empty result snapshot"
    );
}

#[test]
fn AssetExecutionPolicyCoversImmediateAndNoFeasible() {
    let request = AssetRequest::LoadBytes(LoadBytesAssetRequest {
        RequestId: AssetRequestId(5),
        Path: "asset.bin".into(),
    });
    let default_plan = PlanAssetRequestExecution(&request, AssetExecutionConstraints::default());
    assert_eq!(
        default_plan.Mode,
        AssetExecutionMode::ImmediateBytesLoad,
        "default constraints should choose immediate bytes load"
    );

    let disabled_plan = PlanAssetRequestExecution(
        &request,
        AssetExecutionConstraints {
            AllowImmediate: false,
            AllowDeferred: false,
        },
    );
    assert_eq!(
        disabled_plan.Mode,
        AssetExecutionMode::NoAssetExecutionFeasible,
        "when immediate is disabled and deferred disallowed there is no feasible mode"
    );

    let invalid = AssetRequest::LoadBytes(LoadBytesAssetRequest {
        RequestId: AssetRequestId(7),
        Path: "   ".into(),
    });
    let invalid_plan = PlanAssetRequestExecution(&invalid, AssetExecutionConstraints::default());
    assert_eq!(
        invalid_plan.Mode,
        AssetExecutionMode::NoAssetExecutionFeasible,
        "invalid empty path should plan no feasible mode"
    );
    assert!(
        !invalid_plan.Utility.Candidates.is_empty(),
        "utility report should include candidate diagnostics"
    );
}

#[test]
fn DirectExecuteAssetRequestReturnsBytesAndFailures() {
    let path = TempPath("m83_asset_bytes");
    fs::write(&path, [1_u8, 2_u8, 3_u8]).expect("temp file write should succeed");

    let loaded = ExecuteAssetRequest(
        AssetRequest::LoadBytes(LoadBytesAssetRequest {
            RequestId: AssetRequestId(11),
            Path: path.clone(),
        }),
        AssetExecutionConstraints::default(),
    );
    match loaded {
        AssetResult::BytesLoaded(ok) => assert_eq!(
            ok.Bytes,
            vec![1, 2, 3],
            "loaded bytes should match file payload exactly"
        ),
        _ => panic!("expected bytes-loaded result for existing file request"),
    }

    fs::remove_file(&path).expect("temp file removal should succeed");
    let missing = ExecuteAssetRequest(
        AssetRequest::LoadBytes(LoadBytesAssetRequest {
            RequestId: AssetRequestId(12),
            Path: path.clone(),
        }),
        AssetExecutionConstraints::default(),
    );
    match missing {
        AssetResult::LoadFailed(failure) => assert_eq!(
            failure.Kind,
            AssetLoadFailureKind::NotFound,
            "missing file should map to NotFound failure kind"
        ),
        _ => panic!("expected load-failed for missing file request"),
    }
}

#[test]
fn AssetActuatorConsumesRequestStoresResultAndStagesCompletion() {
    let mut board = WorldBlackboard::New();
    let mut mailbox = DwMailbox::New();

    assert!(
        board.Assets.Requests.Snapshot().is_empty(),
        "new world blackboard should initialize empty asset requests"
    );

    let path = TempPath("m83_asset_actuator");
    fs::write(&path, [9_u8, 8_u8]).expect("temp file write should succeed");
    let request_id = board.Assets.Requests.AllocateId();
    board
        .Assets
        .Requests
        .Insert(AssetRequest::LoadBytes(LoadBytesAssetRequest {
            RequestId: request_id,
            Path: path.clone(),
        }));

    ExecuteAssetRequestById(
        request_id,
        &mut board,
        &mut mailbox,
        8301,
        AssetExecutionConstraints::default(),
    )
    .expect("execute by id should succeed for existing file request");
    assert!(
        !board.Assets.Requests.Contains(request_id),
        "actuator should consume request from request store"
    );
    assert!(
        board.Assets.Results.Contains(request_id),
        "actuator should store completion result for request id"
    );
    assert_eq!(
        mailbox.StagedSnapshot(),
        vec![DwMessage::I32(8301, request_id.0 as i32)],
        "actuator should stage id-only completion message"
    );

    mailbox.BeginTick();
    assert_eq!(
        mailbox.ConsumeFirstKind(8301),
        Some(DwMessage::I32(8301, request_id.0 as i32)),
        "completion should become visible next tick"
    );

    fs::remove_file(&path).expect("temp file cleanup should succeed");
}

#[test]
fn AssetActuatorMissingRequestIsErrorWithoutCompletion() {
    let mut board = WorldBlackboard::New();
    let mut mailbox = DwMailbox::New();

    let error = ExecuteAssetRequestById(
        AssetRequestId(404),
        &mut board,
        &mut mailbox,
        8302,
        AssetExecutionConstraints::default(),
    )
    .expect_err("missing request should return execution error");
    assert_eq!(
        error,
        AssetExecutionError::MissingRequest,
        "missing request should return MissingRequest error"
    );
    assert!(
        board.Assets.Results.Snapshot().is_empty(),
        "missing request should not store any result"
    );
    assert!(
        mailbox.StagedSnapshot().is_empty(),
        "missing request should not enqueue completion message"
    );
}

#[test]
fn WorldBlackboardClearResetsAssetStores() {
    let mut board = WorldBlackboard::New();
    let id = board.Assets.Requests.AllocateId();
    board
        .Assets
        .Requests
        .Insert(AssetRequest::LoadBytes(LoadBytesAssetRequest {
            RequestId: id,
            Path: "x".into(),
        }));
    board.Assets.Results.Store(AssetResult::LoadFailed(
        wyrmcoil::Engine::AssetLoadFailure {
            RequestId: id,
            Path: "x".into(),
            Kind: AssetLoadFailureKind::InvalidRequest,
        },
    ));

    board.Clear();
    assert!(
        board.Assets.Requests.Snapshot().is_empty(),
        "clear should reset asset requests"
    );
    assert!(
        board.Assets.Results.Snapshot().is_empty(),
        "clear should reset asset results"
    );
}
