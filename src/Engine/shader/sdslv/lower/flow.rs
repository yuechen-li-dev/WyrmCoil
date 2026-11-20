use super::*;

impl Lowerer<'_> {
    pub(super) fn LowerFlow(&self, flow: &SdslvFlowDecl) -> LowerResult<VdMirFlow> {
        let return_type = self.LowerType(&flow.ReturnType, Some(flow.Span))?;
        if !matches!(
            return_type.Physical(),
            VdMirType::Scalar(_) | VdMirType::FloatVector(_) | VdMirType::Float4x4
        ) {
            return Err(Diagnostic(
                &format!(
                    "flow '{}' returns '{}', which cannot be lowered to an HLSL value function in SDSL-V M13",
                    flow.Name,
                    flow.ReturnType.ToDisplayString()
                ),
                Some(flow.Span),
            ));
        }
        let mut locals = Locals::new();
        let mut parameters = vec![];
        for parameter in &flow.Parameters {
            parameters.push(VdMirParameter {
                Name: parameter.Name.clone(),
                Type: self.LowerType(&parameter.TypeName, Some(flow.Span))?,
            });
            locals.insert(
                parameter.Name.clone(),
                self.Validator.TypeRefToType(&parameter.TypeName),
            );
        }
        let mut board = vec![];
        if let Some(declaration) = &flow.Board {
            for field in &declaration.Fields {
                let ty = self.LowerType(&field.TypeName, Some(field.Span))?;
                if !matches!(
                    ty.Physical(),
                    VdMirType::Scalar(_) | VdMirType::FloatVector(_)
                ) {
                    return Err(Diagnostic(
                        &format!(
                            "flow '{}' board field '{}' of type '{}' cannot be lowered with a default value in SDSL-V M13",
                            flow.Name,
                            field.Name,
                            field.TypeName.ToDisplayString()
                        ),
                        Some(field.Span),
                    ));
                }
                board.push(VdMirField {
                    Name: field.Name.clone(),
                    Type: ty,
                    Role: None,
                    Span: field.Span,
                });
                locals.insert(
                    format!("board.{}", field.Name),
                    self.Validator.TypeRefToType(&field.TypeName),
                );
            }
        }
        let ids: HashMap<_, _> = flow
            .States
            .iter()
            .enumerate()
            .map(|(index, state)| (state.Name.as_str(), index))
            .collect();
        let mut states = vec![];
        for state in &flow.States {
            states.push(self.LowerFlowState(flow, state, &ids, &Scope::Flow(&locals))?);
        }
        let mut marks = vec![Visit::Unseen; states.len()];
        for (index, state) in states.iter().enumerate() {
            if ContainsCycle(index, &states, &mut marks) {
                return Err(Diagnostic(
                    &format!(
                        "flow '{}' contains a state cycle involving '{}'",
                        flow.Name, state.Name,
                    ),
                    None,
                ));
            }
        }
        Ok(VdMirFlow {
            Name: flow.Name.clone(),
            Parameters: parameters,
            ReturnType: return_type,
            Board: board,
            States: states,
        })
    }

    fn LowerFlowState(
        &self,
        flow: &SdslvFlowDecl,
        state: &SdslvFlowState,
        ids: &HashMap<&str, usize>,
        scope: &Scope<'_>,
    ) -> LowerResult<VdMirFlowState> {
        let mut writes = vec![];
        let mut terminator = None;
        for statement in &state.Statements {
            terminator = match statement {
                SdslvFlowStatement::BoardAssign { Field, Value, .. } => {
                    writes.push(VdMirFieldUpdate {
                        Field: Field.clone(),
                        Value: self.LowerValue(Value, scope)?,
                    });
                    continue;
                }
                SdslvFlowStatement::Return(value) => {
                    Some(VdMirFlowTerminator::Return(self.LowerValue(value, scope)?))
                }
                SdslvFlowStatement::Goto(path) => {
                    Some(VdMirFlowTerminator::Goto(FlowTarget(flow, path, ids)?))
                }
                SdslvFlowStatement::When(when) => {
                    let Some(fallback) = &when.ElseAction else {
                        break;
                    };
                    let mut cases = vec![];
                    for case in &when.Cases {
                        let condition = self.LowerValue(&case.Condition, scope)?;
                        let exit = self.LowerFlowExit(flow, &case.Action, ids, scope)?;
                        cases.push((condition, exit));
                    }
                    Some(VdMirFlowTerminator::Branch {
                        Cases: cases,
                        Fallback: self.LowerFlowExit(flow, fallback, ids, scope)?,
                    })
                }
            };
            break;
        }
        let Some(terminator) = terminator else {
            return Err(Diagnostic(
                &format!(
                    "flow '{}' has a non-returning path through state '{}'",
                    flow.Name, state.Name,
                ),
                Some(state.Span),
            ));
        };
        Ok(VdMirFlowState {
            Name: state.Name.clone(),
            Span: state.Span,
            Writes: writes,
            Terminator: terminator,
        })
    }

    fn LowerFlowExit(
        &self,
        flow: &SdslvFlowDecl,
        action: &SdslvFlowAction,
        ids: &HashMap<&str, usize>,
        scope: &Scope<'_>,
    ) -> LowerResult<VdMirFlowExit> {
        match action {
            SdslvFlowAction::Return(value) => {
                Ok(VdMirFlowExit::Return(self.LowerValue(value, scope)?))
            }
            SdslvFlowAction::Goto(path) => Ok(VdMirFlowExit::Goto(FlowTarget(flow, path, ids)?)),
        }
    }
}

fn FlowTarget(
    flow: &SdslvFlowDecl,
    path: &SdslvPath,
    ids: &HashMap<&str, usize>,
) -> LowerResult<usize> {
    let name = PathText(path);
    ids.get(name.as_str()).copied().ok_or_else(|| {
        Diagnostic(
            &format!(
                "goto targets unknown state '{name}' in flow '{}'",
                flow.Name
            ),
            Some(flow.Span),
        )
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Visit {
    Unseen,
    Active,
    Done,
}

fn ContainsCycle(id: usize, states: &[VdMirFlowState], marks: &mut [Visit]) -> bool {
    match marks[id] {
        Visit::Active => return true,
        Visit::Done => return false,
        Visit::Unseen => {}
    }
    marks[id] = Visit::Active;
    let mut visit_exit = |exit: &VdMirFlowExit| match exit {
        VdMirFlowExit::Goto(target) => ContainsCycle(*target, states, marks),
        VdMirFlowExit::Return(_) => false,
    };
    let cyclic = match &states[id].Terminator {
        VdMirFlowTerminator::Goto(target) => ContainsCycle(*target, states, marks),
        VdMirFlowTerminator::Return(_) => false,
        VdMirFlowTerminator::Branch { Cases, Fallback } => {
            Cases.iter().any(|(_, exit)| visit_exit(exit)) || visit_exit(Fallback)
        }
    };
    marks[id] = Visit::Done;
    cyclic
}
