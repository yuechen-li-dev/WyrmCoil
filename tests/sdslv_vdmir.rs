use wyrmcoil::Engine::shader::sdslv::vdmir::*;
use wyrmcoil::Engine::shader::sdslv::*;

#[test]
fn canonical_graphics_uses_typed_interfaces_and_ordered_constructor_arguments() {
    let source = r#"
        type Clip = float4 @space(clip.position);
        stream Input { [location(3)] Position: float3; }
        stream Varyings { Position: Clip; [location(5)] Value: float4; }
        stream Targets { [target(0)] Color: float4; [target(1)] Normal: float4; }
        shader Native {
            stage vertex fn VS(input: Input) -> Varyings {
                return Varyings {
                    Value: float4(1.0, 0.0, 0.0, 1.0);
                    Position: float4(input.Position.x, input.Position.y, input.Position.z, 1.0);
                };
            }
            stage pixel fn PS(input: Varyings) -> Targets {
                return Targets { Color: input.Value; Normal: float4(0.0, 0.0, 1.0, 0.0); };
            }
        }
    "#;
    let syntax = ParseCanonicalSource(source).unwrap();
    let mir = LowerToVdMir(&syntax).unwrap();
    assert_eq!(mir, LowerToVdMir(&syntax).unwrap());
    let aggregates: Vec<_> = mir
        .Items()
        .iter()
        .filter_map(|item| match item {
            VdMirItem::Aggregate(aggregate) => Some(aggregate),
            _ => None,
        })
        .collect();
    assert_eq!(
        aggregates[0].Fields[0].Role,
        Some(VdMirFieldRole::Location(3, VdMirInterpolation::Linear))
    );
    let position = &aggregates[1].Fields[0];
    assert_eq!(position.Role, Some(VdMirFieldRole::ClipPosition));
    assert_eq!(
        position.Type,
        VdMirType::Nominal {
            Space: "clip.position".to_string(),
            Physical: Box::new(VdMirType::FloatVector(4))
        }
    );
    assert!(position.Span.End > position.Span.Start);
    assert_eq!(
        aggregates[2].Fields[1].Role,
        Some(VdMirFieldRole::Target(1))
    );

    let entries: Vec<_> = mir.EntryPoints().collect();
    assert_eq!(entries[0].1.Kind, VdMirEntryKind::Vertex);
    assert_eq!(
        entries[1].1.Kind,
        VdMirEntryKind::Pixel(VdMirPixelOutput::Stream)
    );
    let VdMirStatement::Return { Value } = &entries[0].0.Body[0] else {
        panic!("expected return")
    };
    let VdMirValueKind::Construct { Arguments, .. } = &Value.Kind else {
        panic!("expected constructor")
    };
    let VdMirValueKind::Call {
        Arguments: position_arguments,
        ..
    } = &Arguments[0].Kind
    else {
        panic!("expected source position as first argument")
    };
    assert!(matches!(
        position_arguments[0].Kind,
        VdMirValueKind::FieldAccess { .. }
    ));
    assert_eq!(Arguments[0].Type, VdMirType::FloatVector(4));

    let artifact = BuildShaderArtifact("native.sdslv", &syntax).unwrap();
    assert_eq!(artifact.Hlsl, EmitVdMirHlsl(&mir).unwrap());
    assert_eq!(artifact.EntryPoints[0].Name, entries[0].0.Name);
    assert_eq!(artifact.EntryPoints[1].Name, entries[1].0.Name);
    assert!(artifact.Hlsl.contains("[[vk::location(5)]]"));
    assert!(
        artifact
            .Hlsl
            .contains("Targets Native_PS(Varyings input) {")
    );
}

#[test]
fn legacy_specialization_resolves_the_receiver_call_during_lowering() {
    let source = include_str!("../examples/sdslv/generic_forward_pass.sdslv")
        .replace("mat: TMat", "materialValue: TMat")
        .replace("mat.BaseColor", "materialValue.BaseColor");
    let mir = LowerToVdMir(&ParseSource(&source).unwrap()).unwrap();
    let entries: Vec<_> = mir.EntryPoints().collect();
    assert_eq!(entries.len(), 1);
    let function = entries[0].0;
    assert_eq!(function.Name, "ForwardFlatMaterial_PS");
    assert_eq!(
        function.Parameters[1].Type,
        VdMirType::Aggregate("FlatMaterial".to_string())
    );
    let VdMirStatement::Return { Value } = &function.Body[0] else {
        panic!("expected return")
    };
    let VdMirValueKind::Call { Callee, .. } = &Value.Kind else {
        panic!("expected resolved call")
    };
    assert_eq!(
        Callee.Kind,
        VdMirValueKind::Name("FlatMaterial_BaseColor".to_string())
    );
    assert!(
        EmitVdMirHlsl(&mir)
            .unwrap()
            .contains("return FlatMaterial_BaseColor(s);")
    );
}

#[test]
fn legacy_flow_has_resolved_targets_and_rejects_cycles_before_emission() {
    let source = include_str!("../examples/sdslv/flow_board_writes.sdslv");
    let mir = LowerToVdMir(&ParseSource(source).unwrap()).unwrap();
    let [VdMirItem::Flow(flow)] = mir.Items() else {
        panic!("expected a flow")
    };
    assert_eq!(flow.States[0].Writes.len(), 3);
    assert_eq!(
        flow.States[0].Writes[2].Value.Type,
        VdMirType::Scalar(VdMirScalar::F32)
    );
    let VdMirFlowTerminator::Branch { Cases, Fallback } = &flow.States[0].Terminator else {
        panic!("expected a branch")
    };
    assert_eq!(Cases[0].0.Type, VdMirType::Scalar(VdMirScalar::Bool));
    assert_eq!(Cases[0].1, VdMirFlowExit::Goto(1));
    assert_eq!(*Fallback, VdMirFlowExit::Goto(1));
    assert_eq!(flow.States[1].Name, "Done");

    let cyclic = ParseSource(include_str!("../examples/sdslv/flow_cycle_invalid.sdslv")).unwrap();
    let diagnostics = LowerToVdMir(&cyclic).unwrap_err();
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.Message.contains("state cycle involving 'A'"))
    );
}

#[test]
fn utility_return_temporary_uses_the_mir_function_type() {
    let source = "shader S { fn Pick(x: f32) -> f32 { return when utility { case 1.5 when x > 0.0 score x else 0.25 }; } }";
    let hlsl = CompileSourceToHlsl(source).unwrap();
    assert!(hlsl.contains("float __utility_result0 = 0.25;"));
}
