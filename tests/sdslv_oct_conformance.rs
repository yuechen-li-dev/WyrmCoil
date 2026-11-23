use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use wyrmcoil::Engine::shader::sdslv::*;

const OCT_REVISION: &str = "fa5b4993ca61f79b2ce18d09e9e3bbce88b9b9a0";
const GRAPHICS: &str = "conformance/graphics";
const INVALID: &str = "conformance/invalid";
const VECTOR_ADD: &str = include_str!("fixtures/sdslv/vector-add.sdslv");
const TACTICAL_SKIN: &str =
    include_str!("../integrations/zed/wyrmcoil_view/src/tactical_skin.sdslv");

#[test]
fn vertex_skinning_uses_readonly_float4_storage() {
    let artifact =
        CompileCanonicalSourceToShaderArtifact("tactical_skin.sdslv", TACTICAL_SKIN).unwrap();
    assert!(
        artifact
            .Hlsl
            .contains("StructuredBuffer<float4> __sdslv_storage_JointColumns : register(t0)")
    );
    assert!(artifact.Hlsl.contains("vk::binding(0, 0)"));
    assert!(artifact.Hlsl.contains(
        "TacticalSkin_Project(TacticalSkin_SkinPosition(input, instance.PaletteBase), instance)"
    ));
    let invalid = TACTICAL_SKIN.replace("readonly array<float4>", "readwrite array<float4>");
    let errors =
        CompileCanonicalSourceToShaderArtifact("invalid-skin.sdslv", &invalid).unwrap_err();
    assert!(format!("{errors:?}").contains("graphics storage requires readonly array<float4>"));
}

#[test]
#[ignore = "requires DXC and SPIRV-Tools"]
fn tactical_skin_validates_vulkan_vertex_storage() {
    let artifact =
        CompileCanonicalSourceToShaderArtifact("tactical_skin.sdslv", TACTICAL_SKIN).unwrap();
    let options = DxcOptions {
        ExtraArgs: vec!["-fspv-target-env=vulkan1.3".into()],
        ..Default::default()
    };
    for entry in &artifact.EntryPoints {
        let compiled = CompileArtifactEntryWithDxc(&artifact, entry, &options).unwrap();
        RunSpirvTool(
            "spirv-val",
            &["--target-env", "vulkan1.3", "-"],
            &compiled.OutputBytes,
        );
        if entry.Stage == SdslvShaderStage::Vertex {
            let ir = RunSpirvTool("spirv-dis", &["--no-header", "-"], &compiled.OutputBytes);
            for decoration in [
                "DescriptorSet 0",
                "Binding 0",
                "Location 6",
                "Location 7",
                "Location 8",
                "ArrayStride 16",
            ] {
                assert!(
                    ir.lines().any(|line| line.ends_with(decoration)),
                    "{decoration}"
                );
            }
        }
    }
}

fn OctFixtures() -> PathBuf {
    let root = PathBuf::from(std::env::var_os("WYRMCOIL_OCT_ROOT").expect("set WYRMCOIL_OCT_ROOT"));
    let output = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("inspect the Oct checkout");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), OCT_REVISION);
    root.join("Language/SDSL-V")
}

fn ReadFixture(root: &Path, source: &str) -> String {
    fs::read_to_string(root.join(source)).unwrap_or_else(|error| panic!("{source}: {error}"))
}

#[test]
fn canonical_lexer_preserves_byte_spans_and_foreign_block_boundaries() {
    let source = "let dégradé: u32 = 0XffU << 2u;\nHLSL<f32>(x) { /* } */ if (x) { \"}\"; } }\n";
    let tokens = LexCanonicalSource(source).unwrap();
    assert_eq!(
        tokens[1].Kind,
        SdslvTokenKind::Identifier("dégradé".to_string())
    );
    assert_eq!(&source[tokens[1].Span.Start..tokens[1].Span.End], "dégradé");
    assert_eq!(tokens[2].Span.Column, 12);
    assert_eq!(
        tokens[5].Kind,
        SdslvTokenKind::IntegerLiteral("0XffU".to_string())
    );
    assert_eq!(tokens[6].Kind, SdslvTokenKind::ShiftLeft);
    let foreign = tokens.last().unwrap();
    assert_eq!(foreign.Span.Line, 2);
    assert_eq!(
        &source[foreign.Span.Start..foreign.Span.End],
        "{ /* } */ if (x) { \"}\"; } }"
    );
    assert!(
        matches!(&foreign.Kind, SdslvTokenKind::RawForeignSource(text) if text.contains("if (x)"))
    );
    assert!(LexCanonicalSource("fn f() -> i32 { return a?; }").is_err());
    assert!(LexSource("fn f() -> i32 { return a?; }").is_ok());
    assert!(
        LexSource("let var: i32 = 1;").unwrap().iter().any(|token| {
            matches!(&token.Kind, SdslvTokenKind::Identifier(name) if name == "var")
        })
    );
    assert!(LexSource("\"a\"").is_ok());
}

#[test]
fn native_artifact_uses_oct_operator_precedence_and_explicit_pixel_targets() {
    let source = r#"
        stream Out { [target(1)] Color: float4; }
        shader Native {
            stage pixel fn PS() -> Out {
                var mask: u32 = 0x80u >> 32u;
                if mask & 0x80u == 0x80u and not false {
                    mask = mask | 0x01u;
                }
                return Out { Color: float4(Convert<f32>(mask), 0.0, 0.0, 1.0); };
            }
        }
    "#;
    let artifact = CompileCanonicalSourceToShaderArtifact("native.sdslv", source).unwrap();
    assert_eq!(artifact.EntryPoints[0].Name, "Native_PS");
    for hlsl in [
        "SV_Target1",
        "128u >> (32u & 31u)",
        "(mask & 128u) == 128u && !false",
        "float(mask)",
    ] {
        assert!(
            artifact.Hlsl.contains(hlsl),
            "missing {hlsl} in {}",
            artifact.Hlsl
        );
    }

    let unsupported = "shader S { stage pixel fn PS() -> float4 { return Unknown<f32>()(); } }";
    let diagnostics =
        CompileCanonicalSourceToShaderArtifact("unsupported.sdslv", unsupported).unwrap_err();
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.Message == "unsupported canonical generic call")
    );
}

#[test]
fn oct_vector_add_lowers_to_typed_compute_and_storage() {
    use wyrmcoil::Engine::shader::sdslv::vdmir::*;

    let parsed = ParseCanonicalSource(VECTOR_ADD).unwrap();
    let mir = LowerToVdMir(&parsed).unwrap();
    let bindings: Vec<_> = mir
        .Items()
        .iter()
        .filter_map(|item| match item {
            VdMirItem::StorageBinding(binding) => {
                Some((binding.Binding, binding.Element, binding.Writable))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        bindings,
        [
            (0, VdMirStorageElement::F32, false),
            (1, VdMirStorageElement::F32, false),
            (2, VdMirStorageElement::F32, true),
        ]
    );
    let (function, entry) = mir.EntryPoints().next().unwrap();
    assert_eq!(entry.Kind, VdMirEntryKind::Compute(Some([8, 1, 1])));
    let VdMirStatement::Assign { Target, Value } = &function.Body[1] else {
        panic!("expected an indexed storage assignment");
    };
    assert_eq!(Target.Type, VdMirType::Scalar(VdMirScalar::F32));
    assert_eq!(Value.Type, Target.Type);

    let artifact = BuildShaderArtifact("vector-add.sdslv", &parsed).unwrap();
    let [entry] = artifact.EntryPoints.as_slice() else {
        panic!("expected one compute entry");
    };
    assert_eq!(entry.Name, "VectorAdd_CS");
    assert_eq!(entry.Stage, SdslvShaderStage::Compute);
    assert_eq!(entry.TargetProfile, "cs_6_0");
    for declaration in [
        "[[vk::binding(2, 0)]]",
        "RWStructuredBuffer<float> __sdslv_storage_Output : register(u2)",
        "uint3 DispatchId : SV_DispatchThreadID",
        "[numthreads(8, 1, 1)]",
    ] {
        assert!(artifact.Hlsl.contains(declaration), "missing {declaration}");
    }
}

#[test]
fn canonical_compute_rejects_invalid_storage_and_thread_interfaces() {
    let cases = [
        ("[binding(2)]", "[binding(1)]", "duplicate compute storage"),
        (
            "Output[index] =",
            "Left[index] =",
            "cannot write to readonly compute storage",
        ),
        (
            "Output[index] =",
            "Output[1.5] =",
            "array index must be integer",
        ),
        (
            "Output[index] =",
            "Output[unbound] =",
            "array index must be integer",
        ),
        (
            "Output[index] =",
            "Output =",
            "storage must be accessed by element index",
        ),
        (
            "Left[index] + Right[index]",
            "true",
            "array element assignment type mismatch",
        ),
        (
            "numthreads(8, 1, 1)",
            "numthreads(0, 1, 1)",
            "invalid compute [numthreads",
        ),
        (
            "numthreads(8, 1, 1)",
            "numthreads(64, 32, 1)",
            "invalid compute [numthreads",
        ),
        (
            "DispatchId: uint3",
            "DispatchId: float3",
            "requires an unannotated uint3",
        ),
        (
            "resources ComputeIO;",
            "resources Missing;",
            "must name a storage stream",
        ),
        (
            "array<f32>",
            "array<f32, 16>",
            "compute storage requires array",
        ),
    ];
    for (before, after, expected) in cases {
        let source = VECTOR_ADD.replacen(before, after, 1);
        let errors =
            CompileCanonicalSourceToShaderArtifact("invalid-compute.sdslv", &source).unwrap_err();
        assert!(
            errors.iter().any(|error| error.Message.contains(expected)),
            "{after}: {errors:?}"
        );
    }
    let mixed = format!(
        "{VECTOR_ADD}\nshader Graphics {{ stage pixel fn PS(input: ComputeIO) -> float4 {{ return float4(1.0, 0.0, 0.0, 1.0); }} }}"
    );
    let errors = CompileCanonicalSourceToShaderArtifact("mixed.sdslv", &mixed).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .Message
            .contains("graphics input cannot name a storage stream")),
        "{errors:?}"
    );
}

#[test]
#[ignore = "requires Oct fixtures, DXC, and spirv-val"]
fn oct_storage_compute_validates_with_dxc_and_spirv_tools() {
    let fixtures = OctFixtures();
    let oct = ReadFixture(
        &fixtures,
        "conformance/compute/ComputeNoRegression.sdslvvalid",
    );
    let options = DxcOptions {
        ExtraArgs: vec!["-fspv-target-env=vulkan1.3".into()],
        ..Default::default()
    };
    for (source, name, count) in [
        (VECTOR_ADD, "VectorAdd_CS", 3),
        (oct.as_str(), "ComputeNoRegression_CS", 2),
    ] {
        let artifact = CompileCanonicalSourceToShaderArtifact(name, source).unwrap();
        let spirv = CompileArtifactEntryWithDxc(&artifact, &artifact.EntryPoints[0], &options)
            .unwrap_or_else(|error| panic!("{error:?}\n{}", artifact.Hlsl));
        RunSpirvTool(
            "spirv-val",
            &["--target-env", "vulkan1.3", "-"],
            &spirv.OutputBytes,
        );
        let ir = RunSpirvTool("spirv-dis", &["--no-header", "-"], &spirv.OutputBytes);
        assert!(
            ir.lines()
                .any(|line| line.contains("OpEntryPoint GLCompute") && line.contains(name)),
            "{ir}"
        );
        assert!(ir.contains("BuiltIn GlobalInvocationId"), "{ir}");
        assert!(ir.contains("LocalSize 8 1 1"), "{ir}");
        assert_eq!(
            ir.lines()
                .filter(|line| line.ends_with(" DescriptorSet 0"))
                .count(),
            count
        );
        for binding in 0..count {
            assert!(
                ir.lines()
                    .any(|line| line.ends_with(&format!(" Binding {binding}"))),
                "{ir}"
            );
        }
    }
}

#[test]
#[ignore = "requires the pinned local Oct checkout"]
fn parses_oct_manifest_valid_sources() {
    let fixtures = OctFixtures();
    let cases = [
        "conformance/compute/Float64.sdslvvalid",
        "Modules/ImportedTypeOnly.sdslvvalid",
        "Modules/ImportedGraphics.sdslvvalid",
        "Modules/library/NamespaceImport.sdslvvalid",
        "Modules/TypeAndConcept.sdslvvalid",
        "Modules/ImportedTemplate.sdslvvalid",
        "conformance/compute/BitwiseConstants.sdslvvalid",
        "conformance/compute/OctStyleTemplate.sdslvvalid",
        "conformance/shared/CoordinateSpaces.sdslvvalid",
        "conformance/graphics/CanonicalGraphicsProgram.sdslvvalid",
        "conformance/graphics/MinimalVertex.sdslvvalid",
        "conformance/graphics/MinimalPixel.sdslvvalid",
        "conformance/graphics/MultipleRenderTargets.sdslvvalid",
        "conformance/compute/AttentionSpacePoc.sdslvvalid",
        "conformance/compute/ComputeNoRegression.sdslvvalid",
    ];
    let mut rejected = vec![];
    for path in cases {
        let source = ReadFixture(&fixtures, path);
        match ParseCanonicalSource(&source) {
            Ok(module) => assert!(!module.Declarations.is_empty(), "{path}"),
            Err(diagnostics) => rejected.push(format!("{path}: {:?}", diagnostics[0])),
        }
    }
    assert!(rejected.is_empty(), "{}", rejected.join("\n"));

    let path = format!("{GRAPHICS}/CanonicalGraphicsProgram.sdslvvalid");
    let source = ReadFixture(&fixtures, &path);
    let diagnostics = CompileCanonicalSourceToShaderArtifact(&path, &source).unwrap_err();
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic
            .Message
            .contains("requires canonical template, resource, or compute lowering")
    }));
}

#[test]
#[ignore = "requires the pinned local Oct checkout"]
fn oct_rejection_diagnostics_match_codes_and_start_positions() {
    let fixtures = OctFixtures();
    let cases = [
        ("RemovedFallibleSyntax", "SDSL-V4100", 2, 29),
        ("DeprecatedInterface", "SDSL-V4101", 2, 1),
        ("UninitializedBinding", "SDSL-V3703", 3, 5),
        ("ImmutableMutation", "SDSL-V3701", 4, 5),
        ("ShaderParametersWithoutTemplate", "SDSL-V4303", 2, 13),
        ("TemplateMissingParameters", "SDSL-V4303", 2, 25),
        ("MixedStreamRole", "SDSL-V4102", 2, 1),
        ("LocationCollision", "SDSL-V4105", 4, 5),
        ("MissingVertexClipPosition", "SDSL-V4106", 4, 50),
        ("DuplicateClipPosition", "SDSL-V4107", 4, 38),
        ("InvalidBuiltinForStage", "SDSL-V4109", 2, 16),
        ("WrongBuiltinType", "SDSL-V4110", 2, 47),
        ("VaryingMismatch", "SDSL-V4111", 5, 13),
        ("HexOverflow", "SDSL-V4310", 2, 30),
    ];
    let mut differences = vec![];
    for (fixture, code, line, column) in cases {
        let source = ReadFixture(&fixtures, &format!("{INVALID}/{fixture}.sdslvinvalid"));
        let result = ParseCanonicalSource(&source).and_then(|module| ValidateModule(&module));
        match result {
            Err(diagnostics)
                if diagnostics.iter().any(|diagnostic| {
                    diagnostic.Message.starts_with(code)
                        && diagnostic.Span.Line == line
                        && diagnostic.Span.Column == column
                }) => {}
            other => differences.push(format!(
                "{fixture}: expected {code} at {line}:{column}, got {other:?}"
            )),
        }
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}

#[test]
#[ignore = "requires the pinned local Oct checkout"]
fn parses_oct_guarded_memory_and_flow_board_fixtures() {
    let fixtures = OctFixtures();
    let cases = [
        "M16a/GuardedReadBasic.sdslv",
        "M16a/GuardedWriteBasic.sdslv",
        "M19/GuardWhenBasic.sdslv",
        "M23/FlowBoardBasic.sdslv",
    ];
    for path in cases {
        let source = ReadFixture(&fixtures, path);
        ParseCanonicalSource(&source).unwrap_or_else(|errors| panic!("{path}: {errors:?}"));
    }
}

#[test]
#[ignore = "requires Oct fixtures, DXC, and spirv-val"]
fn oct_graphics_subset_compiles_through_native_hlsl_and_validated_spirv() {
    let fixtures = OctFixtures();
    let cases = [
        ("MinimalVertex", "MinimalVertexProgram_VS", "vs_6_0"),
        ("MinimalPixel", "MinimalPixelProgram_PS", "ps_6_0"),
        ("MultipleRenderTargets", "GBufferProgram_PS", "ps_6_0"),
    ];
    for (fixture, entry, profile) in cases {
        let path = format!("{GRAPHICS}/{fixture}.sdslvvalid");
        let source = ReadFixture(&fixtures, &path);
        let artifact = CompileCanonicalSourceToShaderArtifact(&path, &source).unwrap();
        assert_eq!(artifact.EntryPoints.len(), 1);
        assert_eq!(artifact.EntryPoints[0].Name, entry);
        assert_eq!(artifact.EntryPoints[0].TargetProfile, profile);
        let compiled = CompileArtifactEntryWithDxc(
            &artifact,
            &artifact.EntryPoints[0],
            &DxcOptions::default(),
        )
        .unwrap_or_else(|error| panic!("{path}: {error:?}\n{}", artifact.Hlsl));
        RunSpirvTool(
            "spirv-val",
            &["--target-env", "vulkan1.0", "-"],
            &compiled.OutputBytes,
        );
        let disassembly = RunSpirvTool("spirv-dis", &["--no-header", "-"], &compiled.OutputBytes);
        let execution_model = if profile == "vs_6_0" {
            "Vertex"
        } else {
            "Fragment"
        };
        assert!(disassembly.lines().any(|line| {
            line.contains(&format!("OpEntryPoint {execution_model}"))
                && line.contains(&format!("\"{entry}\""))
        }));
        assert!(
            disassembly
                .lines()
                .any(|line| line.ends_with(" Location 0"))
        );
        if fixture == "MinimalVertex" {
            assert!(
                disassembly
                    .lines()
                    .any(|line| line.ends_with(" BuiltIn Position"))
            );
        }
        if fixture == "MultipleRenderTargets" {
            assert!(
                disassembly
                    .lines()
                    .any(|line| line.ends_with(" Location 1"))
            );
        }
    }
}

fn RunSpirvTool(tool: &str, arguments: &[&str], spirv: &[u8]) -> String {
    let mut child = Command::new(tool)
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("start {tool}: {error}"));
    child.stdin.take().unwrap().write_all(spirv).unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{tool}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap()
}

#[test]
#[ignore = "requires DXC and SPIRV-Tools"]
fn zed_viewport_source_also_compiles_and_validates_with_dxc() {
    let source = include_str!("../integrations/zed/wyrmcoil_view/src/viewport.sdslv");
    let artifact = CompileCanonicalSourceToShaderArtifact("viewport.sdslv", source).unwrap();
    assert_eq!(artifact.EntryPoints.len(), 2);
    for entry in &artifact.EntryPoints {
        let compiled = CompileArtifactEntryWithDxc(&artifact, entry, &DxcOptions::default())
            .unwrap_or_else(|error| panic!("{}: {error:?}", entry.Name));
        RunSpirvTool(
            "spirv-val",
            &["--target-env", "vulkan1.0", "-"],
            &compiled.OutputBytes,
        );
        let disassembly = RunSpirvTool("spirv-dis", &["--no-header", "-"], &compiled.OutputBytes);
        assert!(disassembly.contains(&format!("\"{}\"", entry.Name)));
        if entry.Stage == SdslvShaderStage::Vertex {
            assert!(disassembly.contains("BuiltIn VertexIndex"));
            assert!(disassembly.contains("BuiltIn Position"));
            assert!(
                disassembly
                    .lines()
                    .any(|line| line.ends_with(" Location 1"))
            );
        }
    }
}

#[test]
#[ignore = "requires DXC and SPIRV-Tools"]
fn zed_room_compiles_and_validates_with_dxc() {
    let source = include_str!("../integrations/zed/wyrmcoil_view/src/room.sdslv");
    let artifact = CompileCanonicalSourceToShaderArtifact("room.sdslv", source).unwrap();
    assert_eq!(artifact.EntryPoints.len(), 2);
    for entry in &artifact.EntryPoints {
        let compiled =
            CompileArtifactEntryWithDxc(&artifact, entry, &DxcOptions::default()).unwrap();
        RunSpirvTool(
            "spirv-val",
            &["--target-env", "vulkan1.0", "-"],
            &compiled.OutputBytes,
        );
        let ir = RunSpirvTool("spirv-dis", &["--no-header", "-"], &compiled.OutputBytes);
        assert!(ir.contains(&format!("\"{}\"", entry.Name)));
        if entry.Stage == SdslvShaderStage::Vertex {
            assert!(ir.contains("BuiltIn Position"));
            assert!(ir.lines().any(|line| line.ends_with(" Location 5")));
        }
    }
}
