use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use wyrmcoil::Engine::shader::sdslv::*;

const OCT_REVISION: &str = "fa5b4993ca61f79b2ce18d09e9e3bbce88b9b9a0";
const GRAPHICS: &str = "conformance/graphics";
const INVALID: &str = "conformance/invalid";

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
        }
    }
}
