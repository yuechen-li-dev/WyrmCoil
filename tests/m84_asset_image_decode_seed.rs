#![allow(non_snake_case)]

use std::fs;

use wyrmcoil::Engine::{
    AssetExecutionConstraints, AssetExecutionError, AssetExecutionMode, AssetLoadFailureKind,
    AssetRequest, AssetRequestId, AssetRequestStore, AssetResult, AssetResultStore,
    DecodeImageAssetRequest, ExecuteAssetRequest, ExecuteAssetRequestById, ImageDecodeFailureKind,
    ImageDecodeFormat, LoadBytesAssetRequest, PlanAssetRequestExecution, WorldBlackboard,
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
fn Ppm1x1Rgb(r: u8, g: u8, b: u8) -> Vec<u8> {
    let mut v = b"P6\n1 1\n255\n".to_vec();
    v.extend_from_slice(&[r, g, b]);
    v
}

#[test]
fn DirectDecodePpmToRgba8SuccessAndFailures() {
    let decoded = ExecuteAssetRequest(
        AssetRequest::DecodeImage(DecodeImageAssetRequest {
            RequestId: AssetRequestId(1),
            SourceName: "pixel.ppm".into(),
            Bytes: Ppm1x1Rgb(7, 8, 9),
            Format: ImageDecodeFormat::Ppm,
        }),
        AssetExecutionConstraints::default(),
    );
    match decoded {
        AssetResult::ImageDecoded(image) => {
            assert_eq!(image.Width, 1, "decoded width should be preserved");
            assert_eq!(image.Height, 1, "decoded height should be preserved");
            assert_eq!(
                image.Rgba8,
                vec![7, 8, 9, 255],
                "decoded ppm RGB should expand to RGBA8 with alpha 255"
            );
        }
        _ => panic!("expected ImageDecoded for valid P6 payload"),
    }

    let bad_magic = ExecuteAssetRequest(
        AssetRequest::DecodeImage(DecodeImageAssetRequest {
            RequestId: AssetRequestId(2),
            SourceName: "bad.ppm".into(),
            Bytes: b"P3\n1 1\n255\n\x00\x00\x00".to_vec(),
            Format: ImageDecodeFormat::Ppm,
        }),
        AssetExecutionConstraints::default(),
    );
    match bad_magic {
        AssetResult::DecodeFailed(failure) => assert_eq!(
            failure.Kind,
            ImageDecodeFailureKind::InvalidData,
            "invalid magic should fail as InvalidData"
        ),
        _ => panic!("expected DecodeFailed for invalid magic"),
    }

    let wrong_len = ExecuteAssetRequest(
        AssetRequest::DecodeImage(DecodeImageAssetRequest {
            RequestId: AssetRequestId(3),
            SourceName: "short.ppm".into(),
            Bytes: b"P6\n2 1\n255\n\x01\x02\x03".to_vec(),
            Format: ImageDecodeFormat::Ppm,
        }),
        AssetExecutionConstraints::default(),
    );
    match wrong_len {
        AssetResult::DecodeFailed(failure) => assert_eq!(
            failure.Kind,
            ImageDecodeFailureKind::InvalidData,
            "wrong pixel payload length should fail as InvalidData"
        ),
        _ => panic!("expected DecodeFailed for invalid pixel data length"),
    }
}

#[test]
fn AssetStoresAndPlannerHandleByteAndDecodeRequests() {
    let mut requests = AssetRequestStore::New();
    let id0 = requests.AllocateId();
    let id1 = requests.AllocateId();
    assert_eq!(id0, AssetRequestId(0), "first allocated id should be zero");
    assert_eq!(
        id1,
        AssetRequestId(1),
        "second allocated id should increment deterministically"
    );

    requests.Insert(AssetRequest::LoadBytes(LoadBytesAssetRequest {
        RequestId: id0,
        Path: "a.bin".into(),
    }));
    requests.Insert(AssetRequest::DecodeImage(DecodeImageAssetRequest {
        RequestId: id1,
        SourceName: "b.ppm".into(),
        Bytes: Ppm1x1Rgb(1, 2, 3),
        Format: ImageDecodeFormat::Ppm,
    }));
    assert_eq!(
        requests.Snapshot().len(),
        2,
        "request snapshot should include both request kinds"
    );

    let mut results = AssetResultStore::New();
    results.Store(AssetResult::DecodeFailed(
        wyrmcoil::Engine::ImageDecodeFailure {
            RequestId: id1,
            SourceName: "b.ppm".into(),
            Kind: ImageDecodeFailureKind::InvalidData,
        },
    ));
    assert!(
        results.Contains(id1),
        "result store should contain decode failure result id"
    );

    let decode_request = AssetRequest::DecodeImage(DecodeImageAssetRequest {
        RequestId: AssetRequestId(5),
        SourceName: "seed.ppm".into(),
        Bytes: Ppm1x1Rgb(4, 5, 6),
        Format: ImageDecodeFormat::Ppm,
    });
    let default_plan =
        PlanAssetRequestExecution(&decode_request, AssetExecutionConstraints::default());
    assert_eq!(
        default_plan.Mode,
        AssetExecutionMode::ImmediateImageDecode,
        "decode request should choose immediate image decode under default constraints"
    );

    let disabled_plan = PlanAssetRequestExecution(
        &decode_request,
        AssetExecutionConstraints {
            AllowImmediate: false,
            AllowDeferred: false,
        },
    );
    assert_eq!(
        disabled_plan.Mode,
        AssetExecutionMode::NoAssetExecutionFeasible,
        "when immediate is disabled and deferred disallowed there is no feasible decode mode"
    );
}

#[test]
fn AssetActuatorDecodeAndMissingRequestBehavior() {
    let mut board = WorldBlackboard::New();
    let mut mailbox = DwMailbox::New();

    let decode_ok_id = board.Assets.Requests.AllocateId();
    board
        .Assets
        .Requests
        .Insert(AssetRequest::DecodeImage(DecodeImageAssetRequest {
            RequestId: decode_ok_id,
            SourceName: "ok.ppm".into(),
            Bytes: Ppm1x1Rgb(11, 8, 7),
            Format: ImageDecodeFormat::Ppm,
        }));
    ExecuteAssetRequestById(
        decode_ok_id,
        &mut board,
        &mut mailbox,
        8401,
        AssetExecutionConstraints::default(),
    )
    .expect("decode request should execute by id");
    match board
        .Assets
        .Results
        .Get(decode_ok_id)
        .expect("decode result should be stored")
    {
        AssetResult::ImageDecoded(image) => assert_eq!(
            image.Rgba8,
            vec![11, 8, 7, 255],
            "stored decode result should contain decoded RGBA8"
        ),
        _ => panic!("expected ImageDecoded result for valid decode request"),
    }

    let decode_bad_id = board.Assets.Requests.AllocateId();
    board
        .Assets
        .Requests
        .Insert(AssetRequest::DecodeImage(DecodeImageAssetRequest {
            RequestId: decode_bad_id,
            SourceName: "bad.ppm".into(),
            Bytes: b"P6\n1 1\n255\n\x00\x01".to_vec(),
            Format: ImageDecodeFormat::Ppm,
        }));
    ExecuteAssetRequestById(
        decode_bad_id,
        &mut board,
        &mut mailbox,
        8401,
        AssetExecutionConstraints::default(),
    )
    .expect("invalid decode payload should still execute and store failure result");
    match board
        .Assets
        .Results
        .Get(decode_bad_id)
        .expect("decode failure result should be stored")
    {
        AssetResult::DecodeFailed(failure) => assert_eq!(
            failure.Kind,
            ImageDecodeFailureKind::InvalidData,
            "invalid decode bytes should map to DecodeFailed(InvalidData)"
        ),
        _ => panic!("expected DecodeFailed result for invalid decode request"),
    }

    assert_eq!(
        mailbox.StagedSnapshot(),
        vec![
            DwMessage::I32(8401, decode_ok_id.0 as i32),
            DwMessage::I32(8401, decode_bad_id.0 as i32)
        ],
        "decode actuator should stage id-only completion for success and failure cases"
    );
    mailbox.BeginTick();
    assert_eq!(
        mailbox.ConsumeFirstKind(8401),
        Some(DwMessage::I32(8401, decode_ok_id.0 as i32)),
        "first decode completion should be visible on next tick"
    );
    assert_eq!(
        mailbox.ConsumeFirstKind(8401),
        Some(DwMessage::I32(8401, decode_bad_id.0 as i32)),
        "second decode completion should preserve deterministic staged order"
    );

    let error = ExecuteAssetRequestById(
        AssetRequestId(404),
        &mut board,
        &mut mailbox,
        8402,
        AssetExecutionConstraints::default(),
    )
    .expect_err("missing request should return execution error");
    assert_eq!(
        error,
        AssetExecutionError::MissingRequest,
        "missing request should return MissingRequest error"
    );
    assert!(
        mailbox.StagedSnapshot().is_empty(),
        "missing request should not enqueue completion message"
    );
}

#[test]
fn ByteLoadBehaviorStillWorksAndWorldClearResetsAssetStores() {
    let path = TempPath("m84_asset_bytes");
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
