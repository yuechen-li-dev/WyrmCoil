#![allow(non_snake_case)]

use std::collections::{HashMap, HashSet};

use super::ast::*;
use super::diagnostic::SdslvDiagnostic;
use super::parser::ParseSource;
use super::token::SdslvSpan;

pub fn ValidateSource(source: &str) -> Result<SdslvModule, Vec<SdslvDiagnostic>> {
    let module = ParseSource(source)?;
    ValidateModule(&module)?;
    Ok(module)
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

struct Validator<'a> {
    Module: &'a SdslvModule,
    Diagnostics: Vec<SdslvDiagnostic>,
    TopLevelKinds: HashMap<String, &'static str>,
    InterfaceByName: HashMap<String, &'a SdslvInterfaceDecl>,
    ShaderByName: HashMap<String, &'a SdslvShaderDecl>,
    CompileAliases: HashSet<String>,
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
        }
    }

    fn Validate(&mut self) {
        self.ValidateUses();
        self.BuildTopLevelNames();
        for decl in &self.Module.Declarations {
            match decl {
                SdslvDecl::TypeAlias(alias) => self.ValidateTypeAlias(alias),
                SdslvDecl::Stream(stream) => self.ValidateStream(stream),
                SdslvDecl::Interface(interface) => self.ValidateInterface(interface),
                SdslvDecl::Shader(shader) => self.ValidateShader(shader),
                SdslvDecl::Compile(compile) => self.ValidateCompile(compile),
            }
        }
    }

    fn BuildTopLevelNames(&mut self) {
        for decl in &self.Module.Declarations {
            let (name, kind) = match decl {
                SdslvDecl::TypeAlias(x) => (&x.Name, "type"),
                SdslvDecl::Stream(x) => (&x.Name, "stream"),
                SdslvDecl::Interface(x) => (&x.Name, "interface"),
                SdslvDecl::Shader(x) => (&x.Name, "shader"),
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

    fn ValidateGenericConstraints(&mut self, shader: &SdslvShaderDecl) {
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
                    self.err(&format!(
                        "shader '{}' has where constraint '{}' : '{}' but interface '{}' is unknown",
                        shader.Name, constraint.ParameterName, name, name
                    ));
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
                        self.err(&format!(
                            "shader '{}' override '{}' signature does not match interface declaration",
                            shader.Name, method.Name
                        ));
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
