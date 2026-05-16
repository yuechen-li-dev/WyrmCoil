#![allow(non_snake_case)]

use std::collections::{HashMap, HashSet};

use super::ast::*;
use super::diagnostic::SdslvDiagnostic;
use super::parser::{ParseSource, ParseTestSource};
use super::token::SdslvSpan;

pub fn ValidateSource(source: &str) -> Result<SdslvModule, Vec<SdslvDiagnostic>> {
    let module = ParseSource(source)?;
    ValidateModule(&module)?;
    Ok(module)
}

pub fn ValidateTestSource(source: &str) -> Result<SdslvTestModule, Vec<SdslvDiagnostic>> {
    let module = ParseTestSource(source)?;
    ValidateTestModule(&module)?;
    Ok(module)
}

pub fn ValidateTestModule(module: &SdslvTestModule) -> Result<(), Vec<SdslvDiagnostic>> {
    let mut diagnostics = vec![];
    let mut names = HashSet::new();
    for test in &module.Tests {
        if !names.insert(test.Name.clone()) {
            diagnostics.push(SdslvDiagnostic::New(
                &format!("duplicate test function '{}'", test.Name),
                test.Span,
            ));
        }
        let mut has_fact = false;
        for attribute in &test.Attributes {
            if attribute.Name == "Fact" {
                has_fact = true;
                if !attribute.Arguments.is_empty() {
                    diagnostics.push(SdslvDiagnostic::New(
                        "[Fact] does not accept arguments",
                        attribute.Span,
                    ));
                }
            } else {
                diagnostics.push(SdslvDiagnostic::New(
                    &format!(
                        "unsupported test attribute '{}' in SDSL-V M7a",
                        attribute.Name
                    ),
                    attribute.Span,
                ));
            }
        }
        if has_fact && !test.Parameters.is_empty() {
            diagnostics.push(SdslvDiagnostic::New(
                &format!("[Fact] test '{}' must not declare parameters", test.Name),
                test.Span,
            ));
        }
        for statement in &test.Body.Statements {
            if let SdslvStatement::Expression { Value } = statement {
                ValidateTestExpression(Value, &mut diagnostics);
            }
        }
    }
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}

fn ValidateTestExpression(expression: &SdslvExpression, diagnostics: &mut Vec<SdslvDiagnostic>) {
    let SdslvExpression::Call { Callee, Arguments } = expression else {
        diagnostics.push(SdslvDiagnostic::New(
            "non-assert expression statement is not supported in SDSL-V M7a",
            SdslvSpan {
                Start: 0,
                End: 0,
                Line: 1,
                Column: 1,
            },
        ));
        return;
    };
    let SdslvExpression::FieldAccess { Base, Field } = &**Callee else {
        diagnostics.push(SdslvDiagnostic::New(
            "non-assert expression statement is not supported in SDSL-V M7a",
            SdslvSpan {
                Start: 0,
                End: 0,
                Line: 1,
                Column: 1,
            },
        ));
        return;
    };
    let SdslvExpression::Identifier(base_name) = &**Base else {
        diagnostics.push(SdslvDiagnostic::New(
            "non-assert expression statement is not supported in SDSL-V M7a",
            SdslvSpan {
                Start: 0,
                End: 0,
                Line: 1,
                Column: 1,
            },
        ));
        return;
    };
    if base_name != "Assert" {
        diagnostics.push(SdslvDiagnostic::New(
            "non-assert expression statement is not supported in SDSL-V M7a",
            SdslvSpan {
                Start: 0,
                End: 0,
                Line: 1,
                Column: 1,
            },
        ));
        return;
    }
    let expected = match Field.as_str() {
        "True" => 2,
        "Equals" => 3,
        "Near" => 4,
        _ => {
            diagnostics.push(SdslvDiagnostic::New(
                &format!("unsupported Assert method 'Assert.{}' in SDSL-V M7a", Field),
                SdslvSpan {
                    Start: 0,
                    End: 0,
                    Line: 1,
                    Column: 1,
                },
            ));
            return;
        }
    };
    if Arguments.len() != expected {
        diagnostics.push(SdslvDiagnostic::New(
            &format!("Assert.{} requires {} arguments", Field, expected),
            SdslvSpan {
                Start: 0,
                End: 0,
                Line: 1,
                Column: 1,
            },
        ));
        return;
    }
    if !matches!(Arguments.last(), Some(SdslvExpression::StringLiteral(_))) {
        diagnostics.push(SdslvDiagnostic::New(
            &format!("Assert.{} requires a custom message string argument", Field),
            SdslvSpan {
                Start: 0,
                End: 0,
                Line: 1,
                Column: 1,
            },
        ));
    }
}

pub fn ValidateModule(module: &SdslvModule) -> Result<(), Vec<SdslvDiagnostic>> {
    let mut validator = Validator::New(module);
    validator.Validate();
    if validator.Diagnostics.is_empty() {
        Ok(())
    } else {
        Err(validator.Diagnostics)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TypeRef {
    Named(String),
    Unknown,
}

impl TypeRef {
    fn Name(&self) -> Option<&str> {
        match self {
            TypeRef::Named(name) => Some(name.as_str()),
            TypeRef::Unknown => None,
        }
    }
}

#[derive(Clone)]
struct FunctionSignature {
    Name: String,
    Parameters: Vec<TypeRef>,
    ReturnType: TypeRef,
}

struct Validator<'a> {
    Module: &'a SdslvModule,
    Diagnostics: Vec<SdslvDiagnostic>,
    TopLevelKinds: HashMap<String, &'static str>,
    InterfaceByName: HashMap<String, &'a SdslvInterfaceDecl>,
    ShaderByName: HashMap<String, &'a SdslvShaderDecl>,
    CompileAliases: HashSet<String>,
    StreamByName: HashMap<String, &'a SdslvStreamDecl>,
    MaterialFieldsByShader: HashMap<String, HashMap<String, String>>,
    AliasUnderlyingByName: HashMap<String, String>,
    SemanticAliasNames: HashSet<String>,
    FunctionSignatures: HashMap<String, FunctionSignature>,
}

impl<'a> Validator<'a> {
    fn New(module: &'a SdslvModule) -> Self {
        Self {
            Module: module,
            Diagnostics: vec![],
            TopLevelKinds: HashMap::new(),
            InterfaceByName: HashMap::new(),
            ShaderByName: HashMap::new(),
            CompileAliases: HashSet::new(),
            StreamByName: HashMap::new(),
            MaterialFieldsByShader: HashMap::new(),
            AliasUnderlyingByName: HashMap::new(),
            SemanticAliasNames: HashSet::new(),
            FunctionSignatures: HashMap::new(),
        }
    }

    fn Validate(&mut self) {
        self.ValidateUses();
        self.BuildTopLevelNames();
        self.BuildTypeEnvironment();
        self.BuildFunctionSignatures();
        for decl in &self.Module.Declarations {
            match decl {
                SdslvDecl::TypeAlias(alias) => self.ValidateTypeAlias(alias),
                SdslvDecl::Stream(stream) => self.ValidateStream(stream),
                SdslvDecl::Record(record) => self.ValidateRecord(record),
                SdslvDecl::Interface(interface) => self.ValidateInterface(interface),
                SdslvDecl::Shader(shader) => self.ValidateShader(shader),
                SdslvDecl::Flow(flow) => self.ValidateFlow(flow),
                SdslvDecl::Compile(compile) => self.ValidateCompile(compile),
            }
        }
        self.ValidateFunctionBodies();
    }

    fn BuildTypeEnvironment(&mut self) {
        for decl in &self.Module.Declarations {
            if let SdslvDecl::TypeAlias(alias) = decl {
                let target_name = alias.TargetType.Segments.join(".");
                self.AliasUnderlyingByName
                    .insert(alias.Name.clone(), target_name);
                if alias.SpaceAnnotation.is_some() {
                    self.SemanticAliasNames.insert(alias.Name.clone());
                }
            }
            if let SdslvDecl::Stream(stream) = decl {
                self.StreamByName.insert(stream.Name.clone(), stream);
            }
            if let SdslvDecl::Shader(shader) = decl {
                let mut fields = HashMap::new();
                for field in &shader.MaterialFields {
                    fields.insert(field.Name.clone(), field.TypeName.Segments.join("."));
                }
                self.MaterialFieldsByShader
                    .insert(shader.Name.clone(), fields);
            }
        }
    }

    fn BuildFunctionSignatures(&mut self) {
        for decl in &self.Module.Declarations {
            match decl {
                SdslvDecl::Interface(interface) => {
                    for method in &interface.Methods {
                        self.RegisterFunctionSignature(&method.Name, method);
                    }
                }
                SdslvDecl::Shader(shader) => {
                    for method in &shader.Methods {
                        self.RegisterFunctionSignature(&method.Name, method);
                        self.RegisterFunctionSignature(
                            &format!("{}_{}", shader.Name, method.Name),
                            method,
                        );
                    }
                    for stage_method in &shader.StageMethods {
                        self.RegisterFunctionSignature(&stage_method.Name, stage_method);
                        self.RegisterFunctionSignature(
                            &format!("{}_{}", shader.Name, stage_method.Name),
                            stage_method,
                        );
                    }
                }
                _ => {}
            }
        }
    }

    fn RegisterFunctionSignature(&mut self, name: &str, function: &SdslvFunctionDecl) {
        if self.FunctionSignatures.contains_key(name) {
            return;
        }
        let parameters = function
            .Parameters
            .iter()
            .map(|parameter| self.PathToType(&parameter.TypeName))
            .collect();
        let return_type = self.PathToType(&function.ReturnType);
        self.FunctionSignatures.insert(
            name.to_string(),
            FunctionSignature {
                Name: name.to_string(),
                Parameters: parameters,
                ReturnType: return_type,
            },
        );
    }

    fn BuildTopLevelNames(&mut self) {
        /* unchanged from old */
        for decl in &self.Module.Declarations {
            let (name, kind) = match decl {
                SdslvDecl::TypeAlias(x) => (&x.Name, "type"),
                SdslvDecl::Stream(x) => (&x.Name, "stream"),
                SdslvDecl::Record(x) => (&x.Name, "record"),
                SdslvDecl::Interface(x) => (&x.Name, "interface"),
                SdslvDecl::Shader(x) => (&x.Name, "shader"),
                SdslvDecl::Flow(x) => (&x.Name, "flow"),
                SdslvDecl::Compile(x) => (&x.Alias, "compile"),
            };
            if let Some(existing_kind) = self.TopLevelKinds.insert(name.clone(), kind) {
                self.err(&format!(
                    "duplicate top-level declaration '{}' ({} and {})",
                    name, existing_kind, kind
                ));
            }
            if let SdslvDecl::Interface(interface) = decl {
                self.InterfaceByName
                    .insert(interface.Name.clone(), interface);
            }
            if let SdslvDecl::Shader(shader) = decl {
                self.ShaderByName.insert(shader.Name.clone(), shader);
            }
        }
    }

    fn ValidateUses(&mut self) {
        let mut used = HashSet::new();
        for use_decl in &self.Module.Uses {
            let key = use_decl.Path.Segments.join(".");
            if !used.insert(key.clone()) {
                self.err(&format!("duplicate use declaration '{}'", key));
            }
        }
    }
    fn ValidateTypeAlias(&mut self, alias: &SdslvTypeAliasDecl) {
        if let Some(space) = &alias.SpaceAnnotation {
            if space.Segments.is_empty() {
                self.err(&format!(
                    "type alias '{}' has empty @space() annotation",
                    alias.Name
                ));
            }
        }
    }
    fn ValidateStream(&mut self, stream: &SdslvStreamDecl) {
        let mut names = HashSet::new();
        for field in &stream.Fields {
            if !names.insert(field.Name.clone()) {
                self.err(&format!(
                    "duplicate stream field '{}' in stream '{}'",
                    field.Name, stream.Name
                ));
            }
        }
    }
    fn ValidateRecord(&mut self, record: &SdslvRecordDecl) {
        let mut names = HashSet::new();
        for field in &record.Fields {
            if !names.insert(field.Name.clone()) {
                self.err(&format!(
                    "duplicate record field '{}' in record '{}'",
                    field.Name, record.Name
                ));
            }
        }
    }

    fn ValidateInterface(&mut self, interface: &SdslvInterfaceDecl) {
        let mut names = HashSet::new();
        for method in &interface.Methods {
            if !names.insert(method.Name.clone()) {
                self.err(&format!(
                    "duplicate interface method '{}' in interface '{}'",
                    method.Name, interface.Name
                ));
            }
            if method.Body.is_some() {
                self.err(&format!(
                    "interface method '{}' in interface '{}' cannot have a body",
                    method.Name, interface.Name
                ));
            }
        }
    }

    fn ValidateShader(&mut self, shader: &SdslvShaderDecl) {
        self.ValidateGenericConstraints(shader);
        self.ValidateShaderDuplicates(shader);
        self.ValidateStages(shader);
        self.ValidateImplementsAndOverrides(shader);
    }
    fn ValidateCompile(&mut self, compile: &SdslvCompileDecl) {
        /* keep old logic */
        if let Some(kind) = self.TopLevelKinds.get(&compile.Alias)
            && *kind != "compile"
        {
            self.err(&format!(
                "compile alias '{}' collides with top-level declaration",
                compile.Alias
            ));
        }
        if !self.CompileAliases.insert(compile.Alias.clone()) {
            self.err(&format!("duplicate compile alias '{}'", compile.Alias));
        }
        let generic_name = compile.GenericShader.Segments.join(".");
        let Some(shader) = self.ShaderByName.get(&generic_name) else {
            self.err(&format!(
                "compile references unknown generic shader '{}'",
                generic_name
            ));
            return;
        };
        let shader_name = shader.Name.clone();
        let shader_generics = shader.GenericParameters.clone();
        let shader_constraints = shader.Constraints.clone();
        if shader_generics.is_empty() {
            self.err(&format!("compile target '{}' is not generic", shader.Name));
            return;
        }
        if shader_generics.len() != compile.TypeArguments.len() {
            self.err(&format!(
                "compile '{}' expected {} type arguments but got {}",
                shader_name,
                shader_generics.len(),
                compile.TypeArguments.len()
            ));
            return;
        }
        for arg in &compile.TypeArguments {
            let arg_name = arg.Segments.join(".");
            if !self.ShaderByName.contains_key(&arg_name) {
                self.err(&format!(
                    "compile type argument '{}' is not a known shader",
                    arg_name
                ));
            }
        }
        for constraint in &shader_constraints {
            let Some(index) = shader_generics
                .iter()
                .position(|x| x == &constraint.ParameterName)
            else {
                continue;
            };
            if index >= compile.TypeArguments.len() {
                continue;
            }
            let concrete_name = compile.TypeArguments[index].Segments.join(".");
            let Some(concrete_shader) = self.ShaderByName.get(&concrete_name) else {
                continue;
            };
            let implements = concrete_shader.Implements.clone();
            for bound in &constraint.Bounds {
                if !implements.iter().any(|x| x == bound) {
                    self.err(&format!("compile alias '{}' constraint not satisfied: shader '{}' does not implement '{}'", compile.Alias, concrete_name, bound.Segments.join(".")));
                }
            }
        }
    }

    fn ValidateFunctionBodies(&mut self) {
        for decl in &self.Module.Declarations {
            if let SdslvDecl::Shader(shader) = decl {
                for method in &shader.Methods {
                    self.ValidateFunctionBody(shader, method);
                }
                for stage_method in &shader.StageMethods {
                    self.ValidateFunctionBody(shader, stage_method);
                }
            }
        }
    }
    fn ValidateFlow(&mut self, flow: &SdslvFlowDecl) {
        if flow.Parameters.iter().any(|x| x.Name == "board") {
            self.err(&format!(
                "flow '{}' declares reserved parameter name 'board'",
                flow.Name
            ));
        }
        let board_fields = self.BuildFlowBoardFieldMap(flow);
        if let Some(board) = &flow.Board {
            self.ValidateFlowBoard(flow, board);
        }
        if flow.States.is_empty() {
            self.err(&format!(
                "flow '{}' must declare at least one state",
                flow.Name
            ));
            return;
        }
        let mut names = HashSet::new();
        for state in &flow.States {
            if !names.insert(state.Name.clone()) {
                self.err(&format!(
                    "duplicate state '{}' in flow '{}'",
                    state.Name, flow.Name
                ));
            }
        }
        for state in &flow.States {
            if state.Statements.is_empty() {
                self.err(&format!(
                    "state '{}' in flow '{}' must contain at least one statement",
                    state.Name, flow.Name
                ));
            }
            for statement in &state.Statements {
                self.ValidateFlowStatement(flow, state, statement, &names, &board_fields);
            }
        }
    }

    fn BuildFlowBoardFieldMap(&self, flow: &SdslvFlowDecl) -> HashMap<String, TypeRef> {
        let mut fields = HashMap::new();
        if let Some(board) = &flow.Board {
            for field in &board.Fields {
                fields.insert(field.Name.clone(), self.PathToType(&field.TypeName));
            }
        }
        fields
    }

    fn ValidateFlowBoard(&mut self, flow: &SdslvFlowDecl, board: &SdslvFlowBoard) {
        if board.Fields.is_empty() {
            self.err(&format!(
                "flow '{}' board must declare at least one field",
                flow.Name
            ));
        }
        let mut names = HashSet::new();
        for field in &board.Fields {
            if !names.insert(field.Name.clone()) {
                self.err(&format!(
                    "duplicate board field '{}' in flow '{}'",
                    field.Name, flow.Name
                ));
            }
            let type_name = field.TypeName.Segments.join(".");
            if !self.IsValidFlowBoardTypeName(&type_name) {
                self.err(&format!(
                    "unknown or unsupported board field type '{}' in flow '{}'",
                    type_name, flow.Name
                ));
            }
        }
    }
    fn ValidateFlowStatement(
        &mut self,
        flow: &SdslvFlowDecl,
        state: &SdslvFlowState,
        statement: &SdslvFlowStatement,
        state_names: &HashSet<String>,
        board_fields: &HashMap<String, TypeRef>,
    ) {
        let locals = self.BuildFlowLocals(flow, board_fields);
        let return_type = self.PathToType(&flow.ReturnType);
        match statement {
            SdslvFlowStatement::Goto(path) => self.ValidateFlowGoto(flow, path, state_names),
            SdslvFlowStatement::Return(value) => {
                self.ValidateFlowBoardReads(flow, value, board_fields);
                let actual = self.ResolveFlowExpressionType(&locals, value);
                if let Some((prefix, expected, found)) = self.TypeMismatch(
                    &format!("return type mismatch in flow '{}'", flow.Name),
                    &return_type,
                    &actual,
                ) {
                    self.err(&format!(
                        "{}: expected {}, found {}",
                        prefix, expected, found
                    ));
                }
            }
            SdslvFlowStatement::BoardAssign { Field, Value, .. } => {
                if flow.Board.is_none() {
                    self.err(&format!(
                        "flow '{}' does not declare a board, but statement writes board.{}",
                        flow.Name, Field
                    ));
                    return;
                }
                let Some(expected) = board_fields.get(Field) else {
                    self.err(&format!(
                        "unknown board field '{}' in flow '{}'",
                        Field, flow.Name
                    ));
                    return;
                };
                self.ValidateFlowBoardReads(flow, Value, board_fields);
                let actual = self.ResolveFlowExpressionType(&locals, Value);
                if let Some((prefix, expected_name, found_name)) =
                    self.TypeMismatch("board assignment type mismatch", expected, &actual)
                {
                    self.err(&format!(
                        "{}: expected {}, found {}",
                        prefix, expected_name, found_name
                    ));
                }
            }
            SdslvFlowStatement::When(when) => {
                if when.Cases.is_empty() {
                    self.err(&format!(
                        "guard when in state '{}' must include at least one case",
                        state.Name
                    ));
                }
                if when.ElseAction.is_none() {
                    self.err(&format!(
                        "guard when in state '{}' must include else",
                        state.Name
                    ));
                }
                for case in &when.Cases {
                    self.ValidateFlowBoardReads(flow, &case.Condition, board_fields);
                    let cond_type = self.ResolveFlowExpressionType(&locals, &case.Condition);
                    if cond_type.Name().is_some()
                        && !self.AreCompatible(&TypeRef::Named("bool".to_string()), &cond_type)
                    {
                        self.err(&format!(
                            "guard condition type mismatch in flow '{}': expected bool, found {}",
                            flow.Name,
                            self.TypeName(&cond_type)
                        ));
                    }
                    if let SdslvFlowAction::Goto(path) = &case.Action {
                        self.ValidateFlowGoto(flow, path, state_names);
                    }
                    if let SdslvFlowAction::Return(value) = &case.Action {
                        self.ValidateFlowBoardReads(flow, value, board_fields);
                        let actual = self.ResolveFlowExpressionType(&locals, value);
                        if let Some((prefix, expected, found)) = self.TypeMismatch(
                            &format!("return type mismatch in flow '{}'", flow.Name),
                            &return_type,
                            &actual,
                        ) {
                            self.err(&format!(
                                "{}: expected {}, found {}",
                                prefix, expected, found
                            ));
                        }
                    }
                }
                if let Some(action) = &when.ElseAction
                    && let SdslvFlowAction::Goto(path) = action
                {
                    self.ValidateFlowGoto(flow, path, state_names);
                }
                if let Some(action) = &when.ElseAction
                    && let SdslvFlowAction::Return(value) = action
                {
                    self.ValidateFlowBoardReads(flow, value, board_fields);
                    let actual = self.ResolveFlowExpressionType(&locals, value);
                    if let Some((prefix, expected, found)) = self.TypeMismatch(
                        &format!("return type mismatch in flow '{}'", flow.Name),
                        &return_type,
                        &actual,
                    ) {
                        self.err(&format!(
                            "{}: expected {}, found {}",
                            prefix, expected, found
                        ));
                    }
                }
            }
        }
    }
    fn BuildFlowLocals(
        &self,
        flow: &SdslvFlowDecl,
        board_fields: &HashMap<String, TypeRef>,
    ) -> HashMap<String, TypeRef> {
        let mut locals = HashMap::new();
        for parameter in &flow.Parameters {
            locals.insert(parameter.Name.clone(), self.PathToType(&parameter.TypeName));
        }
        locals.insert(
            "board".to_string(),
            TypeRef::Named("__flow_board".to_string()),
        );
        for (name, ty) in board_fields {
            locals.insert(format!("board.{}", name), ty.clone());
        }
        locals
    }

    fn ValidateFlowBoardReads(
        &mut self,
        flow: &SdslvFlowDecl,
        expression: &SdslvExpression,
        board_fields: &HashMap<String, TypeRef>,
    ) {
        if let Some(field_name) = Self::TryGetBoardFieldRead(expression) {
            if flow.Board.is_none() {
                self.err(&format!(
                    "flow '{}' does not declare a board, but expression references board.{}",
                    flow.Name, field_name
                ));
            } else if !board_fields.contains_key(&field_name) {
                self.err(&format!(
                    "unknown board field '{}' in flow '{}'",
                    field_name, flow.Name
                ));
            }
        }
        match expression {
            SdslvExpression::FieldAccess { Base, .. } => {
                self.ValidateFlowBoardReads(flow, Base, board_fields);
            }
            SdslvExpression::Call { Callee, Arguments } => {
                self.ValidateFlowBoardReads(flow, Callee, board_fields);
                for argument in Arguments {
                    self.ValidateFlowBoardReads(flow, argument, board_fields);
                }
            }
            SdslvExpression::Binary { Left, Right, .. } => {
                self.ValidateFlowBoardReads(flow, Left, board_fields);
                self.ValidateFlowBoardReads(flow, Right, board_fields);
            }
            SdslvExpression::Unary { Operand, .. } => {
                self.ValidateFlowBoardReads(flow, Operand, board_fields);
            }
            _ => {}
        }
    }

    fn TryGetBoardFieldRead(expression: &SdslvExpression) -> Option<String> {
        let SdslvExpression::FieldAccess { Base, Field } = expression else {
            return None;
        };
        let SdslvExpression::Identifier(base_name) = &**Base else {
            return None;
        };
        if base_name == "board" {
            return Some(Field.clone());
        }
        None
    }
    fn ResolveFlowExpressionType(
        &self,
        locals: &HashMap<String, TypeRef>,
        expression: &SdslvExpression,
    ) -> TypeRef {
        match expression {
            SdslvExpression::Identifier(name) => {
                locals.get(name).cloned().unwrap_or(TypeRef::Unknown)
            }
            SdslvExpression::IntegerLiteral(_) => TypeRef::Named("i32".to_string()),
            SdslvExpression::FloatLiteral(_) => TypeRef::Named("float".to_string()),
            SdslvExpression::StringLiteral(_) => TypeRef::Named("string".to_string()),
            SdslvExpression::BoolLiteral(_) => TypeRef::Named("bool".to_string()),
            SdslvExpression::FieldAccess { Base, Field } => {
                if let SdslvExpression::Identifier(base_name) = &**Base
                    && base_name == "board"
                {
                    return locals
                        .get(&format!("board.{}", Field))
                        .cloned()
                        .unwrap_or(TypeRef::Unknown);
                }
                TypeRef::Unknown
            }
            SdslvExpression::Binary {
                Left,
                Operator,
                Right,
            } => {
                let left = self.ResolveFlowExpressionType(locals, Left);
                let right = self.ResolveFlowExpressionType(locals, Right);
                match Operator {
                    SdslvBinaryOperator::Equal
                    | SdslvBinaryOperator::NotEqual
                    | SdslvBinaryOperator::Less
                    | SdslvBinaryOperator::LessEqual
                    | SdslvBinaryOperator::Greater
                    | SdslvBinaryOperator::GreaterEqual => {
                        if self.AreCompatible(&left, &right) {
                            TypeRef::Named("bool".to_string())
                        } else {
                            TypeRef::Unknown
                        }
                    }
                    _ => {
                        if self.AreCompatible(&left, &right) {
                            left
                        } else {
                            TypeRef::Unknown
                        }
                    }
                }
            }
            SdslvExpression::Unary { Operand, .. } => {
                self.ResolveFlowExpressionType(locals, Operand)
            }
            SdslvExpression::Call { .. } => TypeRef::Unknown,
        }
    }
    fn ValidateFlowGoto(
        &mut self,
        flow: &SdslvFlowDecl,
        path: &SdslvPath,
        state_names: &HashSet<String>,
    ) {
        let target = path.Segments.join(".");
        if !state_names.contains(&target) {
            self.err(&format!(
                "goto targets unknown state '{}' in flow '{}'",
                target, flow.Name
            ));
        }
    }

    fn ValidateFunctionBody(&mut self, shader: &SdslvShaderDecl, function: &SdslvFunctionDecl) {
        let Some(body) = &function.Body else {
            return;
        };
        let mut locals = HashMap::new();
        for parameter in &function.Parameters {
            locals.insert(parameter.Name.clone(), self.PathToType(&parameter.TypeName));
        }
        let return_type = self.PathToType(&function.ReturnType);

        for statement in &body.Statements {
            match statement {
                SdslvStatement::Let {
                    Name,
                    TypeName,
                    Initializer,
                } => {
                    let expected = self.PathToType(TypeName);
                    if let Some(init) = Initializer {
                        self.CheckExpressionCalls(shader, &locals, init);
                        let actual = self.ResolveExpressionType(shader, &locals, init);
                        if let Some(msg) = self.TypeMismatch("type mismatch", &expected, &actual) {
                            self.err(&format!("{}: expected {}, found {}", msg.0, msg.1, msg.2));
                        }
                    }
                    locals.insert(Name.clone(), expected);
                }
                SdslvStatement::Assign { Target, Value } => {
                    self.CheckExpressionCalls(shader, &locals, Value);
                    let expected = self.ResolveAssignmentTargetType(shader, &locals, Target);
                    let actual = self.ResolveExpressionType(shader, &locals, Value);
                    if let Some(msg) =
                        self.TypeMismatch("assignment type mismatch", &expected, &actual)
                    {
                        self.err(&format!("{}: expected {}, found {}", msg.0, msg.1, msg.2));
                    }
                }
                SdslvStatement::Return { Value } => {
                    self.CheckExpressionCalls(shader, &locals, Value);
                    let actual = self.ResolveExpressionType(shader, &locals, Value);
                    if let Some(msg) = self.TypeMismatch(
                        &format!("return type mismatch in {}", function.Name),
                        &return_type,
                        &actual,
                    ) {
                        self.err(&format!("{}: expected {}, found {}", msg.0, msg.1, msg.2));
                    }
                }
                SdslvStatement::Expression { Value } => {
                    self.CheckExpressionCalls(shader, &locals, Value);
                }
                SdslvStatement::Empty => {}
            }
        }
    }

    fn ResolveAssignmentTargetType(
        &self,
        shader: &SdslvShaderDecl,
        locals: &HashMap<String, TypeRef>,
        expression: &SdslvExpression,
    ) -> TypeRef {
        self.ResolveExpressionType(shader, locals, expression)
    }

    fn CheckExpressionCalls(
        &mut self,
        shader: &SdslvShaderDecl,
        locals: &HashMap<String, TypeRef>,
        expression: &SdslvExpression,
    ) {
        match expression {
            SdslvExpression::Call { Callee, Arguments } => {
                if let SdslvExpression::Identifier(name) = &**Callee
                    && let Some(signature) = self.FunctionSignatures.get(name).cloned()
                {
                    for (index, argument) in Arguments.iter().enumerate() {
                        if index >= signature.Parameters.len() {
                            break;
                        }
                        let actual = self.ResolveExpressionType(shader, locals, argument);
                        if let Some((_, expected_name, actual_name)) =
                            self.TypeMismatch("", &signature.Parameters[index], &actual)
                        {
                            self.err(&format!(
                                "argument {} of {} expects {}, found {}",
                                index + 1,
                                signature.Name,
                                expected_name,
                                actual_name
                            ));
                        }
                    }
                }
                for argument in Arguments {
                    self.CheckExpressionCalls(shader, locals, argument);
                }
            }
            SdslvExpression::FieldAccess { Base, .. } => {
                self.CheckExpressionCalls(shader, locals, Base)
            }
            SdslvExpression::Binary { Left, Right, .. } => {
                self.CheckExpressionCalls(shader, locals, Left);
                self.CheckExpressionCalls(shader, locals, Right);
            }
            SdslvExpression::Unary { Operand, .. } => {
                self.CheckExpressionCalls(shader, locals, Operand)
            }
            _ => {}
        }
    }

    fn ResolveExpressionType(
        &self,
        shader: &SdslvShaderDecl,
        locals: &HashMap<String, TypeRef>,
        expression: &SdslvExpression,
    ) -> TypeRef {
        match expression {
            SdslvExpression::Identifier(name) => {
                if let Some(local) = locals.get(name) {
                    return local.clone();
                }
                if let Some(materials) = self.MaterialFieldsByShader.get(&shader.Name)
                    && let Some(t) = materials.get(name)
                {
                    return TypeRef::Named(t.clone());
                }
                TypeRef::Unknown
            }
            SdslvExpression::IntegerLiteral(_) => TypeRef::Named("i32".to_string()),
            SdslvExpression::FloatLiteral(_) => TypeRef::Named("float".to_string()),
            SdslvExpression::StringLiteral(_) => TypeRef::Named("string".to_string()),
            SdslvExpression::BoolLiteral(_) => TypeRef::Named("bool".to_string()),
            SdslvExpression::FieldAccess { Base, Field } => {
                let base_type = self.ResolveExpressionType(shader, locals, Base);
                self.ResolveFieldType(&base_type, Field)
            }
            SdslvExpression::Call { Callee, Arguments } => {
                self.ResolveCallType(shader, locals, Callee, Arguments)
            }
            SdslvExpression::Binary { Left, Right, .. } => {
                let l = self.ResolveExpressionType(shader, locals, Left);
                let r = self.ResolveExpressionType(shader, locals, Right);
                if self.AreCompatible(&l, &r) {
                    l
                } else {
                    TypeRef::Unknown
                }
            }
            SdslvExpression::Unary { Operand, .. } => {
                self.ResolveExpressionType(shader, locals, Operand)
            }
        }
    }

    fn ResolveCallType(
        &self,
        shader: &SdslvShaderDecl,
        locals: &HashMap<String, TypeRef>,
        callee: &SdslvExpression,
        arguments: &[SdslvExpression],
    ) -> TypeRef {
        let SdslvExpression::Identifier(callee_name) = callee else {
            return TypeRef::Unknown;
        };
        if Self::IsBuiltinCtor(callee_name) {
            return TypeRef::Named(callee_name.clone());
        }
        let Some(signature) = self.FunctionSignatures.get(callee_name) else {
            return TypeRef::Unknown;
        };
        if signature.Parameters.len() != arguments.len() {
            return TypeRef::Unknown;
        }
        for (index, argument) in arguments.iter().enumerate() {
            let actual = self.ResolveExpressionType(shader, locals, argument);
            if let Some(msg) = self.TypeMismatch(
                &format!(
                    "argument {} of {} expects {}",
                    index + 1,
                    signature.Name,
                    self.TypeName(&signature.Parameters[index])
                ),
                &signature.Parameters[index],
                &actual,
            ) {
                let _ = msg;
            }
        }
        signature.ReturnType.clone()
    }

    fn ResolveFieldType(&self, base_type: &TypeRef, field: &str) -> TypeRef {
        let Some(base_name) = base_type.Name() else {
            return TypeRef::Unknown;
        };
        if let Some(stream) = self.StreamByName.get(base_name)
            && let Some(stream_field) = stream.Fields.iter().find(|x| x.Name == field)
        {
            return self.PathToType(&stream_field.TypeName);
        }
        let underlying = self.ResolveUnderlyingName(base_name);
        if let Some(swizzle) = Self::ResolveSwizzleType(&underlying, field) {
            return TypeRef::Named(swizzle);
        }
        TypeRef::Unknown
    }

    fn ResolveSwizzleType(base: &str, field: &str) -> Option<String> {
        let base_dim = match base {
            "float2" => 2,
            "float3" => 3,
            "float4" => 4,
            _ => return None,
        };
        if !field.chars().all(|c| matches!(c, 'x' | 'y' | 'z' | 'w')) {
            return None;
        }
        let max = field
            .chars()
            .map(|c| match c {
                'x' => 1,
                'y' => 2,
                'z' => 3,
                'w' => 4,
                _ => 0,
            })
            .max()
            .unwrap_or(0);
        if max > base_dim {
            return None;
        }
        match field.len() {
            1 => Some("float".to_string()),
            2..=4 => Some(format!("float{}", field.len())),
            _ => None,
        }
    }

    fn PathToType(&self, path: &SdslvPath) -> TypeRef {
        TypeRef::Named(path.Segments.join("."))
    }
    fn TypeName(&self, t: &TypeRef) -> String {
        t.Name().unwrap_or("<unknown>").to_string()
    }

    fn TypeMismatch(
        &self,
        prefix: &str,
        expected: &TypeRef,
        actual: &TypeRef,
    ) -> Option<(String, String, String)> {
        if self.AreCompatible(expected, actual) {
            return None;
        }
        let (Some(e), Some(a)) = (expected.Name(), actual.Name()) else {
            return None;
        };
        Some((prefix.to_string(), e.to_string(), a.to_string()))
    }

    fn AreCompatible(&self, expected: &TypeRef, actual: &TypeRef) -> bool {
        let (Some(e), Some(a)) = (expected.Name(), actual.Name()) else {
            return true;
        };
        if e == a {
            return true;
        }
        let e_sem = self.SemanticAliasNames.contains(e);
        let a_sem = self.SemanticAliasNames.contains(a);
        let e_under = self.ResolveUnderlyingName(e);
        let a_under = self.ResolveUnderlyingName(a);
        if e_sem && !a_sem {
            return e_under == a_under;
        }
        if e_sem || a_sem {
            return false;
        }
        e_under == a_under
    }

    fn ResolveUnderlyingName(&self, name: &str) -> String {
        let mut current = Self::NormalizeBuiltinTypeName(name);
        let mut guard = 0;
        while let Some(next) = self.AliasUnderlyingByName.get(&current) {
            current = Self::NormalizeBuiltinTypeName(next);
            guard += 1;
            if guard > 64 {
                break;
            }
        }
        current
    }
    fn NormalizeBuiltinTypeName(name: &str) -> String {
        match name {
            "f32" => "float".to_string(),
            "i32" => "i32".to_string(),
            "u32" => "u32".to_string(),
            _ => name.to_string(),
        }
    }

    fn IsValidFlowBoardTypeName(&self, name: &str) -> bool {
        if Self::IsBuiltinFlowBoardType(name) {
            return true;
        }
        self.AliasUnderlyingByName.contains_key(name)
    }

    fn IsBuiltinFlowBoardType(name: &str) -> bool {
        matches!(
            name,
            "bool" | "i32" | "u32" | "f32" | "float" | "float2" | "float3" | "float4" | "float4x4"
        )
    }
    fn IsBuiltinCtor(name: &str) -> bool {
        matches!(name, "float2" | "float3" | "float4" | "float4x4")
    }

    fn ValidateGenericConstraints(&mut self, shader: &SdslvShaderDecl) {
        /* unchanged */
        let mut generic_names = HashSet::new();
        for generic in &shader.GenericParameters {
            if !generic_names.insert(generic.clone()) {
                self.err(&format!(
                    "shader '{}' has duplicate generic parameter '{}'",
                    shader.Name, generic
                ));
            }
        }
        let mut seen_pairs = HashSet::new();
        for constraint in &shader.Constraints {
            if !generic_names.contains(&constraint.ParameterName) {
                self.err(&format!(
                    "shader '{}' has where constraint on unknown generic parameter '{}'",
                    shader.Name, constraint.ParameterName
                ));
            }
            for bound in &constraint.Bounds {
                let name = bound.Segments.join(".");
                if !self.InterfaceByName.contains_key(&name) {
                    self.err(&format!("shader '{}' has where constraint '{}' : '{}' but interface '{}' is unknown", shader.Name, constraint.ParameterName, name, name));
                }
                let pair = format!("{}::{}", constraint.ParameterName, name);
                if !seen_pairs.insert(pair) {
                    self.err(&format!(
                        "shader '{}' repeats where constraint '{}' : '{}'",
                        shader.Name, constraint.ParameterName, name
                    ));
                }
            }
        }
    }
    fn ValidateShaderDuplicates(&mut self, shader: &SdslvShaderDecl) {
        let mut material = HashSet::new();
        for field in &shader.MaterialFields {
            if !material.insert(field.Name.clone()) {
                self.err(&format!(
                    "duplicate material field '{}' in shader '{}'",
                    field.Name, shader.Name
                ));
            }
        }
        let mut methods = HashSet::new();
        for method in &shader.Methods {
            if !methods.insert(method.Name.clone()) {
                self.err(&format!(
                    "duplicate shader method '{}' in shader '{}'",
                    method.Name, shader.Name
                ));
            }
        }
        let mut stages = HashSet::new();
        for stage_method in &shader.StageMethods {
            if !stages.insert(stage_method.Name.clone()) {
                self.err(&format!(
                    "duplicate stage method '{}' in shader '{}'",
                    stage_method.Name, shader.Name
                ));
            }
            if methods.contains(&stage_method.Name) {
                self.err(&format!(
                    "shader '{}' has method '{}' that collides with stage method name",
                    shader.Name, stage_method.Name
                ));
            }
        }
    }
    fn ValidateStages(&mut self, shader: &SdslvShaderDecl) {
        for method in &shader.StageMethods {
            match method.Stage.as_deref() {
                Some("vertex") | Some("pixel") | Some("compute") => {}
                Some(stage_name) => {
                    self.err(&format!(
                        "stage '{}' is not supported in SDSL-V v0",
                        stage_name
                    ));
                }
                None => self.err(&format!(
                    "stage method '{}' in shader '{}' is missing stage name",
                    method.Name, shader.Name
                )),
            }
            if method.Body.is_none() {
                self.err(&format!(
                    "stage method '{}' in shader '{}' must have a body",
                    method.Name, shader.Name
                ));
            }
        }
    }
    fn ValidateImplementsAndOverrides(&mut self, shader: &SdslvShaderDecl) {
        let mut required_methods: HashMap<String, &SdslvFunctionDecl> = HashMap::new();
        for iface in &shader.Implements {
            let iface_name = iface.Segments.join(".");
            let interface = match self.InterfaceByName.get(&iface_name) {
                Some(x) => *x,
                None => {
                    self.err(&format!(
                        "shader '{}' implements unknown interface '{}'",
                        shader.Name, iface_name
                    ));
                    continue;
                }
            };
            for method in &interface.Methods {
                required_methods.insert(method.Name.clone(), method);
            }
        }
        for required in required_methods.values() {
            let shader_method = shader.Methods.iter().find(|m| m.Name == required.Name);
            match shader_method {
                None => self.err(&format!(
                    "shader '{}' implements interface method '{}' but does not override it",
                    shader.Name, required.Name
                )),
                Some(method) => {
                    if !method.IsOverride {
                        self.err(&format!(
                            "shader '{}' method '{}' must be marked override",
                            shader.Name, method.Name
                        ));
                    }
                    if !Self::SameSignature(method, required) {
                        self.err(&format!("shader '{}' override '{}' signature does not match interface declaration", shader.Name, method.Name));
                    }
                }
            }
        }
        for method in &shader.Methods {
            if method.IsOverride && !required_methods.contains_key(&method.Name) {
                self.err(&format!(
                    "shader '{}' override method '{}' is not declared by implemented interfaces",
                    shader.Name, method.Name
                ));
            }
        }
    }
    fn SameSignature(left: &SdslvFunctionDecl, right: &SdslvFunctionDecl) -> bool {
        if left.Parameters.len() != right.Parameters.len() {
            return false;
        }
        if left.ReturnType != right.ReturnType {
            return false;
        }
        for index in 0..left.Parameters.len() {
            if left.Parameters[index].TypeName != right.Parameters[index].TypeName {
                return false;
            }
        }
        true
    }
    fn err(&mut self, message: &str) {
        self.Diagnostics
            .push(SdslvDiagnostic::New(message, Self::UnknownSpan()));
    }
    fn UnknownSpan() -> SdslvSpan {
        SdslvSpan {
            Start: 0,
            End: 0,
            Line: 1,
            Column: 1,
        }
    }
}
