#![allow(non_snake_case)]

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::Engine::render::sampler::SamplerPlan;
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialTomlAsset {
    pub Asset: MaterialAssetHeader,
    pub Material: MaterialHeader,
    pub Nodes: Vec<MaterialNode>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialAssetHeader {
    pub Type: String,
    pub Version: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialHeader {
    pub Name: String,
    pub Output: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialNode {
    pub Id: String,
    pub Kind: String,
    pub Inputs: BTreeMap<String, String>,
    pub Params: BTreeMap<String, MaterialParamValue>,
    pub Editor: BTreeMap<String, MaterialParamValue>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MaterialParamValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Array(Vec<MaterialParamValue>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum MaterialTomlParseError {
    EmptySourceName,
    EmptySource,
    TomlSyntax { SourceName: String, Message: String },
    UnsupportedParamShape { NodeId: String, Field: String },
}

#[derive(Clone, Debug, PartialEq)]
pub enum MaterialTomlValidationError {
    InvalidAssetType {
        Actual: String,
    },
    UnsupportedVersion {
        Version: u32,
    },
    EmptyMaterialName,
    EmptyMaterialOutput,
    MaterialOutputMissingNode {
        Output: String,
    },
    NoNodes,
    EmptyNodeId,
    InvalidNodeIdFormat {
        NodeId: String,
    },
    DuplicateNodeId {
        NodeId: String,
    },
    EmptyNodeKind {
        NodeId: String,
    },
    EmptyInputName {
        NodeId: String,
    },
    UnknownInputReference {
        NodeId: String,
        InputName: String,
        ReferencedId: String,
    },
    SelfInputReference {
        NodeId: String,
        InputName: String,
    },
    GraphCycleDetected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MaterialValueType {
    F32,
    Float2,
    Float3,
    Float4,
    Texture2D,
    Surface,
    Unknown,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialGraphSemantics {
    pub OutputNodeId: String,
    pub NodeTypes: BTreeMap<String, MaterialValueType>,
    pub TopologicalNodeIds: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MaterialSemanticError {
    Structural(MaterialTomlValidationError),
    UnsupportedNodeKind {
        NodeId: String,
        Kind: String,
    },
    MissingInput {
        NodeId: String,
        Input: String,
    },
    UnknownInput {
        NodeId: String,
        Input: String,
    },
    MissingParam {
        NodeId: String,
        Param: String,
    },
    UnknownParam {
        NodeId: String,
        Param: String,
    },
    ParamTypeMismatch {
        NodeId: String,
        Param: String,
        Expected: String,
        Found: String,
    },
    OperationTypeMismatch {
        NodeId: String,
        Message: String,
    },
    OutputMustBeSurface {
        OutputNodeId: String,
        Found: MaterialValueType,
    },
}

impl MaterialTomlAsset {
    pub fn NodeById(&self, id: &str) -> Option<&MaterialNode> {
        self.Nodes.iter().find(|node| node.Id == id)
    }

    pub fn NodeIds(&self) -> Vec<String> {
        self.Nodes.iter().map(|node| node.Id.clone()).collect()
    }

    pub fn TopologicalNodeIds(&self) -> Result<Vec<String>, MaterialTomlValidationError> {
        ValidateMaterialTomlAsset(self)?;

        let mut indegree: BTreeMap<String, usize> = BTreeMap::new();
        let mut dependents: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for node in &self.Nodes {
            indegree.insert(node.Id.clone(), node.Inputs.len());
        }
        for node in &self.Nodes {
            for dependency in node.Inputs.values() {
                dependents
                    .entry(dependency.clone())
                    .or_default()
                    .push(node.Id.clone());
            }
        }

        let mut ready: VecDeque<String> = self
            .Nodes
            .iter()
            .filter(|node| *indegree.get(&node.Id).unwrap_or(&0) == 0)
            .map(|node| node.Id.clone())
            .collect();
        let mut ordered: Vec<String> = Vec::with_capacity(self.Nodes.len());

        while let Some(node_id) = ready.pop_front() {
            ordered.push(node_id.clone());
            if let Some(children) = dependents.get(&node_id) {
                for child in children {
                    if let Some(child_degree) = indegree.get_mut(child) {
                        *child_degree = child_degree.saturating_sub(1);
                        if *child_degree == 0 {
                            ready.push_back(child.clone());
                        }
                    }
                }
            }
        }

        if ordered.len() != self.Nodes.len() {
            return Err(MaterialTomlValidationError::GraphCycleDetected);
        }

        Ok(ordered)
    }
}

#[derive(Deserialize)]
struct RawMaterialTomlAsset {
    asset: RawMaterialAssetHeader,
    material: RawMaterialHeader,
    #[serde(default, rename = "node")]
    nodes: Vec<RawMaterialNode>,
}

#[derive(Deserialize)]
struct RawMaterialAssetHeader {
    r#type: String,
    version: u32,
}

#[derive(Deserialize)]
struct RawMaterialHeader {
    name: String,
    output: String,
}

#[derive(Deserialize, Default)]
struct RawMaterialNode {
    #[serde(default)]
    id: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    inputs: BTreeMap<String, String>,
    #[serde(default)]
    params: BTreeMap<String, toml::Value>,
    #[serde(default)]
    editor: BTreeMap<String, toml::Value>,
}

pub fn ParseMaterialToml(
    source_name: &str,
    source: &str,
) -> Result<MaterialTomlAsset, MaterialTomlParseError> {
    if source_name.trim().is_empty() {
        return Err(MaterialTomlParseError::EmptySourceName);
    }
    if source.trim().is_empty() {
        return Err(MaterialTomlParseError::EmptySource);
    }

    let raw: RawMaterialTomlAsset =
        toml::from_str(source).map_err(|error| MaterialTomlParseError::TomlSyntax {
            SourceName: source_name.to_string(),
            Message: error.to_string(),
        })?;

    let mut nodes = Vec::with_capacity(raw.nodes.len());
    for raw_node in raw.nodes {
        let params = ConvertParamMap(&raw_node.id, "params", raw_node.params)?;
        let editor = ConvertParamMap(&raw_node.id, "editor", raw_node.editor)?;
        nodes.push(MaterialNode {
            Id: raw_node.id,
            Kind: raw_node.kind,
            Inputs: raw_node.inputs,
            Params: params,
            Editor: editor,
        });
    }

    Ok(MaterialTomlAsset {
        Asset: MaterialAssetHeader {
            Type: raw.asset.r#type,
            Version: raw.asset.version,
        },
        Material: MaterialHeader {
            Name: raw.material.name,
            Output: raw.material.output,
        },
        Nodes: nodes,
    })
}

fn ConvertParamMap(
    node_id: &str,
    field: &str,
    map: BTreeMap<String, toml::Value>,
) -> Result<BTreeMap<String, MaterialParamValue>, MaterialTomlParseError> {
    let mut converted = BTreeMap::new();
    for (key, value) in map {
        converted.insert(
            key,
            ConvertParamValue(node_id, field, value).map_err(|_| {
                MaterialTomlParseError::UnsupportedParamShape {
                    NodeId: node_id.to_string(),
                    Field: field.to_string(),
                }
            })?,
        );
    }
    Ok(converted)
}

fn ConvertParamValue(
    node_id: &str,
    field: &str,
    value: toml::Value,
) -> Result<MaterialParamValue, MaterialTomlParseError> {
    let _ = (node_id, field);
    match value {
        toml::Value::String(v) => Ok(MaterialParamValue::String(v)),
        toml::Value::Integer(v) => Ok(MaterialParamValue::Integer(v)),
        toml::Value::Float(v) => Ok(MaterialParamValue::Float(v)),
        toml::Value::Boolean(v) => Ok(MaterialParamValue::Boolean(v)),
        toml::Value::Array(values) => {
            let mut converted = Vec::with_capacity(values.len());
            for item in values {
                converted.push(ConvertParamValue(node_id, field, item)?);
            }
            Ok(MaterialParamValue::Array(converted))
        }
        toml::Value::Datetime(_) | toml::Value::Table(_) => {
            Err(MaterialTomlParseError::UnsupportedParamShape {
                NodeId: node_id.to_string(),
                Field: field.to_string(),
            })
        }
    }
}

pub fn ValidateMaterialTomlAsset(
    asset: &MaterialTomlAsset,
) -> Result<(), MaterialTomlValidationError> {
    if asset.Asset.Type != "material" {
        return Err(MaterialTomlValidationError::InvalidAssetType {
            Actual: asset.Asset.Type.clone(),
        });
    }
    if asset.Asset.Version != 1 {
        return Err(MaterialTomlValidationError::UnsupportedVersion {
            Version: asset.Asset.Version,
        });
    }
    if asset.Material.Name.trim().is_empty() {
        return Err(MaterialTomlValidationError::EmptyMaterialName);
    }
    if asset.Material.Output.trim().is_empty() {
        return Err(MaterialTomlValidationError::EmptyMaterialOutput);
    }
    if asset.Nodes.is_empty() {
        return Err(MaterialTomlValidationError::NoNodes);
    }

    let mut known_ids = BTreeSet::new();
    for node in &asset.Nodes {
        if node.Id.trim().is_empty() {
            return Err(MaterialTomlValidationError::EmptyNodeId);
        }
        if !IsValidNodeId(&node.Id) {
            return Err(MaterialTomlValidationError::InvalidNodeIdFormat {
                NodeId: node.Id.clone(),
            });
        }
        if !known_ids.insert(node.Id.clone()) {
            return Err(MaterialTomlValidationError::DuplicateNodeId {
                NodeId: node.Id.clone(),
            });
        }
        if node.Kind.trim().is_empty() {
            return Err(MaterialTomlValidationError::EmptyNodeKind {
                NodeId: node.Id.clone(),
            });
        }
    }

    if !known_ids.contains(&asset.Material.Output) {
        return Err(MaterialTomlValidationError::MaterialOutputMissingNode {
            Output: asset.Material.Output.clone(),
        });
    }

    for node in &asset.Nodes {
        for (input_name, dependency_id) in &node.Inputs {
            if input_name.trim().is_empty() {
                return Err(MaterialTomlValidationError::EmptyInputName {
                    NodeId: node.Id.clone(),
                });
            }
            if dependency_id == &node.Id {
                return Err(MaterialTomlValidationError::SelfInputReference {
                    NodeId: node.Id.clone(),
                    InputName: input_name.clone(),
                });
            }
            if !known_ids.contains(dependency_id) {
                return Err(MaterialTomlValidationError::UnknownInputReference {
                    NodeId: node.Id.clone(),
                    InputName: input_name.clone(),
                    ReferencedId: dependency_id.clone(),
                });
            }
        }
    }

    let _ = DetectTopologicalOrder(asset)?;
    Ok(())
}

fn DetectTopologicalOrder(
    asset: &MaterialTomlAsset,
) -> Result<Vec<String>, MaterialTomlValidationError> {
    let mut indegree: BTreeMap<String, usize> = BTreeMap::new();
    let mut dependents: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for node in &asset.Nodes {
        indegree.insert(node.Id.clone(), node.Inputs.len());
    }
    for node in &asset.Nodes {
        for dependency in node.Inputs.values() {
            dependents
                .entry(dependency.clone())
                .or_default()
                .push(node.Id.clone());
        }
    }

    let mut ready: VecDeque<String> = asset
        .Nodes
        .iter()
        .filter(|node| *indegree.get(&node.Id).unwrap_or(&0) == 0)
        .map(|node| node.Id.clone())
        .collect();
    let mut ordered = Vec::with_capacity(asset.Nodes.len());

    while let Some(node_id) = ready.pop_front() {
        ordered.push(node_id.clone());
        if let Some(children) = dependents.get(&node_id) {
            for child in children {
                if let Some(child_degree) = indegree.get_mut(child) {
                    *child_degree = child_degree.saturating_sub(1);
                    if *child_degree == 0 {
                        ready.push_back(child.clone());
                    }
                }
            }
        }
    }

    if ordered.len() != asset.Nodes.len() {
        return Err(MaterialTomlValidationError::GraphCycleDetected);
    }

    Ok(ordered)
}

fn IsValidNodeId(value: &str) -> bool {
    let mut chars = value.chars();
    let first = match chars.next() {
        Some(ch) => ch,
        None => return false,
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}

pub fn ValidateMaterialTomlSemantics(
    asset: &MaterialTomlAsset,
) -> Result<MaterialGraphSemantics, MaterialSemanticError> {
    ValidateMaterialTomlAsset(asset).map_err(MaterialSemanticError::Structural)?;
    let all_topo = asset
        .TopologicalNodeIds()
        .map_err(MaterialSemanticError::Structural)?;
    let reachable = ReachableNodeIdsFromOutput(asset);
    let node_by_id: BTreeMap<&str, &MaterialNode> =
        asset.Nodes.iter().map(|n| (n.Id.as_str(), n)).collect();
    let mut node_types: BTreeMap<String, MaterialValueType> = BTreeMap::new();
    let topo_reachable: Vec<String> = all_topo
        .into_iter()
        .filter(|id| reachable.contains(id))
        .collect();

    for node_id in &topo_reachable {
        let node = node_by_id
            .get(node_id.as_str())
            .expect("reachable node should exist");
        let inferred = InferNodeType(node, &node_types, &node_by_id)?;
        node_types.insert(node_id.clone(), inferred);
    }

    let output_type = node_types
        .get(&asset.Material.Output)
        .copied()
        .unwrap_or(MaterialValueType::Unknown);
    if output_type != MaterialValueType::Surface {
        return Err(MaterialSemanticError::OutputMustBeSurface {
            OutputNodeId: asset.Material.Output.clone(),
            Found: output_type,
        });
    }

    Ok(MaterialGraphSemantics {
        OutputNodeId: asset.Material.Output.clone(),
        NodeTypes: node_types,
        TopologicalNodeIds: topo_reachable,
    })
}

fn ReachableNodeIdsFromOutput(asset: &MaterialTomlAsset) -> BTreeSet<String> {
    let mut visited = BTreeSet::new();
    let mut stack = vec![asset.Material.Output.clone()];
    let by_id: BTreeMap<&str, &MaterialNode> =
        asset.Nodes.iter().map(|n| (n.Id.as_str(), n)).collect();
    while let Some(node_id) = stack.pop() {
        if !visited.insert(node_id.clone()) {
            continue;
        }
        if let Some(node) = by_id.get(node_id.as_str()) {
            for dep in node.Inputs.values() {
                stack.push(dep.clone());
            }
        }
    }
    visited
}

fn InferNodeType(
    node: &MaterialNode,
    known_types: &BTreeMap<String, MaterialValueType>,
    node_by_id: &BTreeMap<&str, &MaterialNode>,
) -> Result<MaterialValueType, MaterialSemanticError> {
    match node.Kind.as_str() {
        "constant_f32" => {
            RequireExactInputs(node, &[])?;
            RequireExactParams(node, &["value"])?;
            let value = node.Params.get("value").expect("value required");
            if !IsNumericParam(value) {
                return Err(MaterialSemanticError::ParamTypeMismatch {
                    NodeId: node.Id.clone(),
                    Param: "value".to_string(),
                    Expected: "numeric".to_string(),
                    Found: MaterialParamTypeName(value).to_string(),
                });
            }
            Ok(MaterialValueType::F32)
        }
        "constant_float4" => {
            RequireExactInputs(node, &[])?;
            RequireExactParams(node, &["value"])?;
            let value = node.Params.get("value").expect("value required");
            if AsFloat4Param(value).is_none() {
                return Err(MaterialSemanticError::ParamTypeMismatch {
                    NodeId: node.Id.clone(),
                    Param: "value".to_string(),
                    Expected: "float4".to_string(),
                    Found: MaterialParamTypeName(value).to_string(),
                });
            }
            Ok(MaterialValueType::Float4)
        }
        "texture2d" => {
            RequireExactInputs(node, &[])?;
            RequireAllowedParams(node, &["path", "color_space"])?;
            let path = match node.Params.get("path") {
                Some(MaterialParamValue::String(v)) if !v.trim().is_empty() => v,
                Some(v) => {
                    return Err(MaterialSemanticError::ParamTypeMismatch {
                        NodeId: node.Id.clone(),
                        Param: "path".to_string(),
                        Expected: "non-empty string".to_string(),
                        Found: MaterialParamTypeName(v).to_string(),
                    });
                }
                None => {
                    return Err(MaterialSemanticError::MissingParam {
                        NodeId: node.Id.clone(),
                        Param: "path".to_string(),
                    });
                }
            };
            let _ = path;
            if let Some(color_space) = node.Params.get("color_space") {
                match color_space {
                    MaterialParamValue::String(v) if v == "srgb" || v == "linear" => {}
                    MaterialParamValue::String(_) => {
                        return Err(MaterialSemanticError::ParamTypeMismatch {
                            NodeId: node.Id.clone(),
                            Param: "color_space".to_string(),
                            Expected: "'srgb' or 'linear'".to_string(),
                            Found: "other string".to_string(),
                        });
                    }
                    other => {
                        return Err(MaterialSemanticError::ParamTypeMismatch {
                            NodeId: node.Id.clone(),
                            Param: "color_space".to_string(),
                            Expected: "string".to_string(),
                            Found: MaterialParamTypeName(other).to_string(),
                        });
                    }
                }
            }
            Ok(MaterialValueType::Float4)
        }
        "multiply" => {
            RequireExactParams(node, &[])?;
            RequireExactInputs(node, &["a", "b"])?;
            let a = InputType(node, "a", known_types, node_by_id)?;
            let b = InputType(node, "b", known_types, node_by_id)?;
            match (a, b) {
                (MaterialValueType::F32, MaterialValueType::F32) => Ok(MaterialValueType::F32),
                (MaterialValueType::Float4, MaterialValueType::Float4)
                | (MaterialValueType::Float4, MaterialValueType::F32)
                | (MaterialValueType::F32, MaterialValueType::Float4) => {
                    Ok(MaterialValueType::Float4)
                }
                _ => Err(MaterialSemanticError::OperationTypeMismatch {
                    NodeId: node.Id.clone(),
                    Message: format!("multiply unsupported types: {a:?} * {b:?}"),
                }),
            }
        }
        "add" => {
            RequireExactParams(node, &[])?;
            RequireExactInputs(node, &["a", "b"])?;
            let a = InputType(node, "a", known_types, node_by_id)?;
            let b = InputType(node, "b", known_types, node_by_id)?;
            match (a, b) {
                (MaterialValueType::F32, MaterialValueType::F32) => Ok(MaterialValueType::F32),
                (MaterialValueType::Float4, MaterialValueType::Float4) => {
                    Ok(MaterialValueType::Float4)
                }
                _ => Err(MaterialSemanticError::OperationTypeMismatch {
                    NodeId: node.Id.clone(),
                    Message: format!("add unsupported types: {a:?} + {b:?}"),
                }),
            }
        }
        "lerp" => {
            RequireExactParams(node, &[])?;
            RequireExactInputs(node, &["a", "b", "t"])?;
            let a = InputType(node, "a", known_types, node_by_id)?;
            let b = InputType(node, "b", known_types, node_by_id)?;
            let t = InputType(node, "t", known_types, node_by_id)?;
            if a != b {
                return Err(MaterialSemanticError::OperationTypeMismatch {
                    NodeId: node.Id.clone(),
                    Message: format!("lerp a/b types must match: {a:?} vs {b:?}"),
                });
            }
            if !(a == MaterialValueType::F32 || a == MaterialValueType::Float4) {
                return Err(MaterialSemanticError::OperationTypeMismatch {
                    NodeId: node.Id.clone(),
                    Message: format!("lerp supports F32 or Float4, got {a:?}"),
                });
            }
            if t != MaterialValueType::F32 {
                return Err(MaterialSemanticError::OperationTypeMismatch {
                    NodeId: node.Id.clone(),
                    Message: format!("lerp t must be F32, got {t:?}"),
                });
            }
            Ok(a)
        }
        "standard_surface" => {
            RequireExactParams(node, &[])?;
            RequireAllowedInputs(node, &["base_color", "roughness", "metallic"])?;
            if !node.Inputs.contains_key("base_color") {
                return Err(MaterialSemanticError::MissingInput {
                    NodeId: node.Id.clone(),
                    Input: "base_color".to_string(),
                });
            }
            if InputType(node, "base_color", known_types, node_by_id)? != MaterialValueType::Float4
            {
                return Err(MaterialSemanticError::OperationTypeMismatch {
                    NodeId: node.Id.clone(),
                    Message: "standard_surface.base_color must be Float4".to_string(),
                });
            }
            if node.Inputs.contains_key("roughness")
                && InputType(node, "roughness", known_types, node_by_id)? != MaterialValueType::F32
            {
                return Err(MaterialSemanticError::OperationTypeMismatch {
                    NodeId: node.Id.clone(),
                    Message: "standard_surface.roughness must be F32".to_string(),
                });
            }
            if node.Inputs.contains_key("metallic")
                && InputType(node, "metallic", known_types, node_by_id)? != MaterialValueType::F32
            {
                return Err(MaterialSemanticError::OperationTypeMismatch {
                    NodeId: node.Id.clone(),
                    Message: "standard_surface.metallic must be F32".to_string(),
                });
            }
            Ok(MaterialValueType::Surface)
        }
        "output" => {
            RequireExactParams(node, &[])?;
            RequireExactInputs(node, &["color"])?;
            if InputType(node, "color", known_types, node_by_id)? != MaterialValueType::Float4 {
                return Err(MaterialSemanticError::OperationTypeMismatch {
                    NodeId: node.Id.clone(),
                    Message: "output.color must be Float4".to_string(),
                });
            }
            Ok(MaterialValueType::Surface)
        }
        _ => Err(MaterialSemanticError::UnsupportedNodeKind {
            NodeId: node.Id.clone(),
            Kind: node.Kind.clone(),
        }),
    }
}

fn InputType(
    node: &MaterialNode,
    input: &str,
    known_types: &BTreeMap<String, MaterialValueType>,
    _: &BTreeMap<&str, &MaterialNode>,
) -> Result<MaterialValueType, MaterialSemanticError> {
    let source_id = node
        .Inputs
        .get(input)
        .ok_or_else(|| MaterialSemanticError::MissingInput {
            NodeId: node.Id.clone(),
            Input: input.to_string(),
        })?;
    known_types.get(source_id).copied().ok_or_else(|| {
        MaterialSemanticError::OperationTypeMismatch {
            NodeId: node.Id.clone(),
            Message: format!("input '{input}' dependency '{source_id}' type unavailable"),
        }
    })
}
fn RequireExactInputs(node: &MaterialNode, expected: &[&str]) -> Result<(), MaterialSemanticError> {
    RequireAllowedInputs(node, expected)?;
    for name in expected {
        if !node.Inputs.contains_key(*name) {
            return Err(MaterialSemanticError::MissingInput {
                NodeId: node.Id.clone(),
                Input: (*name).to_string(),
            });
        }
    }
    Ok(())
}
fn RequireAllowedInputs(
    node: &MaterialNode,
    allowed: &[&str],
) -> Result<(), MaterialSemanticError> {
    for name in node.Inputs.keys() {
        if !allowed.contains(&name.as_str()) {
            return Err(MaterialSemanticError::UnknownInput {
                NodeId: node.Id.clone(),
                Input: name.clone(),
            });
        }
    }
    Ok(())
}
fn RequireExactParams(node: &MaterialNode, expected: &[&str]) -> Result<(), MaterialSemanticError> {
    RequireAllowedParams(node, expected)?;
    for name in expected {
        if !node.Params.contains_key(*name) {
            return Err(MaterialSemanticError::MissingParam {
                NodeId: node.Id.clone(),
                Param: (*name).to_string(),
            });
        }
    }
    Ok(())
}
fn RequireAllowedParams(
    node: &MaterialNode,
    allowed: &[&str],
) -> Result<(), MaterialSemanticError> {
    for name in node.Params.keys() {
        if !allowed.contains(&name.as_str()) {
            return Err(MaterialSemanticError::UnknownParam {
                NodeId: node.Id.clone(),
                Param: name.clone(),
            });
        }
    }
    Ok(())
}
fn MaterialParamTypeName(value: &MaterialParamValue) -> &'static str {
    match value {
        MaterialParamValue::String(_) => "string",
        MaterialParamValue::Integer(_) => "integer",
        MaterialParamValue::Float(_) => "float",
        MaterialParamValue::Boolean(_) => "boolean",
        MaterialParamValue::Array(_) => "array",
    }
}
fn IsNumericParam(value: &MaterialParamValue) -> bool {
    matches!(
        value,
        MaterialParamValue::Integer(_) | MaterialParamValue::Float(_)
    )
}
fn AsFloat4Param(value: &MaterialParamValue) -> Option<[f32; 4]> {
    let MaterialParamValue::Array(values) = value else {
        return None;
    };
    if values.len() != 4 {
        return None;
    }
    let mut out = [0.0; 4];
    for (idx, v) in values.iter().enumerate() {
        out[idx] = match v {
            MaterialParamValue::Integer(i) => *i as f32,
            MaterialParamValue::Float(f) => *f as f32,
            _ => return None,
        };
    }
    Some(out)
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialSdslvSource {
    pub SourceName: String,
    pub Source: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MaterialSdslvCodegenError {
    Structural(MaterialTomlValidationError),
    Semantic(MaterialSemanticError),
    UnsupportedNodeKind { NodeId: String, Kind: String },
    UnsupportedType { NodeId: String, TypeName: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialResourceRequirements {
    pub MaterialName: String,
    pub Textures: Vec<MaterialTextureRequirement>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialTextureRequirement {
    pub NodeId: String,
    pub SanitizedName: String,
    pub AssetPath: String,
    pub ColorSpace: MaterialTextureColorSpace,
    pub Sampler: MaterialSamplerRequirement,
    pub BindingName: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialTextureColorSpace {
    Srgb,
    Linear,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialSamplerRequirement {
    pub SamplerName: String,
    pub Plan: SamplerPlan,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MaterialResourceRequirementError {
    Structural(MaterialTomlValidationError),
    Semantic(MaterialSemanticError),
    MissingTexturePath { NodeId: String },
    InvalidTextureColorSpace { NodeId: String, Value: String },
    SamplerPlanBuildFailed { NodeId: String, SamplerName: String },
}

pub fn CollectMaterialResourceRequirements(
    asset: &MaterialTomlAsset,
) -> Result<MaterialResourceRequirements, MaterialResourceRequirementError> {
    let semantics = ValidateMaterialTomlSemantics(asset).map_err(|error| match error {
        MaterialSemanticError::Structural(validation) => {
            MaterialResourceRequirementError::Structural(validation)
        }
        semantic => MaterialResourceRequirementError::Semantic(semantic),
    })?;
    CollectMaterialResourceRequirementsFromSemantics(asset, &semantics)
}

pub fn CollectMaterialResourceRequirementsFromSemantics(
    asset: &MaterialTomlAsset,
    semantics: &MaterialGraphSemantics,
) -> Result<MaterialResourceRequirements, MaterialResourceRequirementError> {
    ValidateMaterialTomlAsset(asset).map_err(MaterialResourceRequirementError::Structural)?;
    let mut used_sanitized_names = BTreeSet::new();
    let mut textures = Vec::new();
    for node_id in &semantics.TopologicalNodeIds {
        let node = asset.NodeById(node_id).expect("semantic node should exist");
        if node.Kind != "texture2d" {
            continue;
        }
        let sanitized_name = SanitizeSdslvIdentifier(node_id, &mut used_sanitized_names);
        let asset_path = TexturePath(node)?;
        let color_space = TextureColorSpace(node)?;
        let binding_name = format!("tex_{sanitized_name}");
        let sampler_name = format!("samp_{sanitized_name}");
        let plan = SamplerPlan::DefaultColor(&sampler_name).map_err(|_| {
            MaterialResourceRequirementError::SamplerPlanBuildFailed {
                NodeId: node_id.clone(),
                SamplerName: sampler_name.clone(),
            }
        })?;
        textures.push(MaterialTextureRequirement {
            NodeId: node_id.clone(),
            SanitizedName: sanitized_name,
            AssetPath: asset_path,
            ColorSpace: color_space,
            Sampler: MaterialSamplerRequirement {
                SamplerName: sampler_name,
                Plan: plan,
            },
            BindingName: binding_name,
        });
    }

    Ok(MaterialResourceRequirements {
        MaterialName: asset.Material.Name.clone(),
        Textures: textures,
    })
}

fn TexturePath(node: &MaterialNode) -> Result<String, MaterialResourceRequirementError> {
    match node.Params.get("path") {
        Some(MaterialParamValue::String(path)) if !path.trim().is_empty() => Ok(path.clone()),
        _ => Err(MaterialResourceRequirementError::MissingTexturePath {
            NodeId: node.Id.clone(),
        }),
    }
}

fn TextureColorSpace(
    node: &MaterialNode,
) -> Result<MaterialTextureColorSpace, MaterialResourceRequirementError> {
    match node.Params.get("color_space") {
        None => Ok(MaterialTextureColorSpace::Srgb),
        Some(MaterialParamValue::String(value)) if value == "srgb" => {
            Ok(MaterialTextureColorSpace::Srgb)
        }
        Some(MaterialParamValue::String(value)) if value == "linear" => {
            Ok(MaterialTextureColorSpace::Linear)
        }
        Some(MaterialParamValue::String(value)) => {
            Err(MaterialResourceRequirementError::InvalidTextureColorSpace {
                NodeId: node.Id.clone(),
                Value: value.clone(),
            })
        }
        _ => Err(MaterialResourceRequirementError::InvalidTextureColorSpace {
            NodeId: node.Id.clone(),
            Value: "non-string".to_string(),
        }),
    }
}

pub fn GenerateMaterialSdslv(
    asset: &MaterialTomlAsset,
) -> Result<MaterialSdslvSource, MaterialSdslvCodegenError> {
    let semantics =
        ValidateMaterialTomlSemantics(asset).map_err(MaterialSdslvCodegenError::Semantic)?;
    GenerateMaterialSdslvFromSemantics(asset, &semantics)
}

pub fn GenerateMaterialSdslvFromSemantics(
    asset: &MaterialTomlAsset,
    semantics: &MaterialGraphSemantics,
) -> Result<MaterialSdslvSource, MaterialSdslvCodegenError> {
    ValidateMaterialTomlAsset(asset).map_err(MaterialSdslvCodegenError::Structural)?;
    let mut used = BTreeSet::new();
    let mut names = BTreeMap::new();
    for node_id in &semantics.TopologicalNodeIds {
        names.insert(node_id.clone(), SanitizeSdslvIdentifier(node_id, &mut used));
    }
    let mut lines = vec![
        "record MaterialSurface {".to_string(),
        "    BaseColor: float4;".to_string(),
        "    Roughness: f32;".to_string(),
        "    Metallic: f32;".to_string(),
        "}".to_string(),
        "".to_string(),
    ];
    lines.push("shader GeneratedMaterial {".to_string());
    for node_id in &semantics.TopologicalNodeIds {
        let node = asset.NodeById(node_id).expect("semantic node should exist");
        if node.Kind == "texture2d" {
            let name = names.get(node_id).expect("mapped");
            lines.push(format!("    fn SampleTexture2D_{name}() -> float4 {{"));
            lines.push("        return float4(1.0, 1.0, 1.0, 1.0);".to_string());
            lines.push("    }".to_string());
        }
    }
    lines.push("    fn EvaluateMaterial() -> MaterialSurface {".to_string());
    let output = asset
        .NodeById(&semantics.OutputNodeId)
        .expect("output exists");
    for node_id in &semantics.TopologicalNodeIds {
        let node = asset.NodeById(node_id).expect("semantic node should exist");
        if node.Id == output.Id || node.Kind == "standard_surface" || node.Kind == "output" {
            continue;
        }
        let name = names.get(node_id).expect("mapped");
        match node.Kind.as_str() {
            "constant_f32" => {
                let v = node
                    .Params
                    .get("value")
                    .and_then(AsF32Param)
                    .expect("semantic value");
                lines.push(format!("        let {name}: f32 = {};", FormatF32(v)));
            }
            "constant_float4" => {
                let v = node
                    .Params
                    .get("value")
                    .and_then(AsFloat4Param)
                    .expect("semantic value");
                lines.push(format!(
                    "        let {name}: float4 = float4({}, {}, {}, {});",
                    FormatF32(v[0]),
                    FormatF32(v[1]),
                    FormatF32(v[2]),
                    FormatF32(v[3])
                ));
            }
            "texture2d" => lines.push(format!(
                "        let {name}: float4 = SampleTexture2D_{name}();"
            )),
            "multiply" | "add" | "lerp" => {
                let a = names.get(node.Inputs.get("a").expect("a")).expect("mapped");
                let b = names.get(node.Inputs.get("b").expect("b")).expect("mapped");
                let ty = match semantics
                    .NodeTypes
                    .get(node_id)
                    .copied()
                    .unwrap_or(MaterialValueType::Unknown)
                {
                    MaterialValueType::F32 => "f32",
                    MaterialValueType::Float4 => "float4",
                    other => {
                        return Err(MaterialSdslvCodegenError::UnsupportedType {
                            NodeId: node.Id.clone(),
                            TypeName: format!("{other:?}"),
                        });
                    }
                };
                let expr = if node.Kind == "multiply" {
                    format!("{a} * {b}")
                } else if node.Kind == "add" {
                    format!("{a} + {b}")
                } else {
                    let t = names.get(node.Inputs.get("t").expect("t")).expect("mapped");
                    format!("{a} + ({b} - {a}) * {t}")
                };
                lines.push(format!("        let {name}: {ty} = {expr};"));
            }
            _ => {
                return Err(MaterialSdslvCodegenError::UnsupportedNodeKind {
                    NodeId: node.Id.clone(),
                    Kind: node.Kind.clone(),
                });
            }
        }
    }
    let (base_color, roughness, metallic) = if output.Kind == "standard_surface" {
        (
            names
                .get(output.Inputs.get("base_color").expect("base_color"))
                .expect("mapped")
                .clone(),
            output
                .Inputs
                .get("roughness")
                .and_then(|id| names.get(id))
                .cloned()
                .unwrap_or_else(|| "0.5".to_string()),
            output
                .Inputs
                .get("metallic")
                .and_then(|id| names.get(id))
                .cloned()
                .unwrap_or_else(|| "0.0".to_string()),
        )
    } else {
        (
            names
                .get(output.Inputs.get("color").expect("output color"))
                .expect("mapped")
                .clone(),
            "0.5".to_string(),
            "0.0".to_string(),
        )
    };
    lines.push("        let surface: MaterialSurface;".to_string());
    lines.push(format!("        surface.BaseColor = {base_color};"));
    lines.push(format!("        surface.Roughness = {roughness};"));
    lines.push(format!("        surface.Metallic = {metallic};"));
    lines.push("        return surface;".to_string());
    lines.push("    }".to_string());
    lines.push("}".to_string());
    Ok(MaterialSdslvSource {
        SourceName: format!("{}.generated.sdslv", asset.Material.Name),
        Source: lines.join("\n"),
    })
}

fn SanitizeSdslvIdentifier(source: &str, used: &mut BTreeSet<String>) -> String {
    let mut out: String = source
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if out.is_empty() {
        out = "node".to_string();
    }
    if out.starts_with(|ch: char| ch.is_ascii_digit()) {
        out = format!("n_{out}");
    }
    if ["shader", "record", "fn", "let", "return"].contains(&out.as_str()) {
        out.push_str("_v");
    }
    let base = out.clone();
    let mut i = 1;
    while used.contains(&out) {
        out = format!("{base}_{i}");
        i += 1;
    }
    used.insert(out.clone());
    out
}

fn AsF32Param(value: &MaterialParamValue) -> Option<f32> {
    match value {
        MaterialParamValue::Integer(i) => Some(*i as f32),
        MaterialParamValue::Float(f) => Some(*f as f32),
        _ => None,
    }
}

fn FormatF32(value: f32) -> String {
    let mut text = format!("{value:.6}");
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.push('0');
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn FlatMaterialSource() -> &'static str {
        include_str!("../../examples/materials/flat_magenta.toml")
    }
    fn TextureTintSource() -> &'static str {
        include_str!("../../examples/materials/texture_tint.toml")
    }

    #[test]
    fn ParseValidMaterials() {
        let flat = ParseMaterialToml("flat_magenta.toml", FlatMaterialSource())
            .expect("flat material should parse");
        let tint = ParseMaterialToml("texture_tint.toml", TextureTintSource())
            .expect("texture+tint material should parse");
        assert_eq!(flat.Nodes.len(), 2, "flat material should have two nodes");
        assert_eq!(
            tint.Nodes.len(),
            4,
            "texture+tint material should have four nodes"
        );
    }

    #[test]
    fn ParseRejectsEmptyInputsAndMalformedToml() {
        assert_eq!(
            ParseMaterialToml("", "x = 1").unwrap_err(),
            MaterialTomlParseError::EmptySourceName,
            "empty source name should be rejected"
        );
        assert_eq!(
            ParseMaterialToml("a.toml", "   ").unwrap_err(),
            MaterialTomlParseError::EmptySource,
            "empty source should be rejected"
        );
        match ParseMaterialToml(
            "bad.toml",
            "[asset
type='material'",
        ) {
            Err(MaterialTomlParseError::TomlSyntax { .. }) => {}
            other => panic!("expected syntax error, got {other:?}"),
        }
    }

    #[test]
    fn ParseParamShapes() {
        let source = r#"
[asset]
type = "material"
version = 1
[material]
name = "Params"
output = "n"
[[node]]
id = "n"
kind = "constant_float4"
[node.params]
s = "x"
i = 7
f = 0.5
b = true
a = [1, 2.0, "three", false]
"#;
        let parsed =
            ParseMaterialToml("params.toml", source).expect("mixed param values should parse");
        assert!(
            matches!(parsed.Nodes[0].Params.get("s"), Some(MaterialParamValue::String(v)) if v == "x")
        );
        assert!(matches!(
            parsed.Nodes[0].Params.get("i"),
            Some(MaterialParamValue::Integer(7))
        ));
        assert!(
            matches!(parsed.Nodes[0].Params.get("f"), Some(MaterialParamValue::Float(v)) if (*v - 0.5).abs() < 0.0001)
        );
        assert!(matches!(
            parsed.Nodes[0].Params.get("b"),
            Some(MaterialParamValue::Boolean(true))
        ));
        assert!(
            matches!(parsed.Nodes[0].Params.get("a"), Some(MaterialParamValue::Array(values)) if values.len() == 4)
        );
    }

    #[test]
    fn ValidateRulesAndHelpers() {
        let mut asset =
            ParseMaterialToml("flat.toml", FlatMaterialSource()).expect("flat should parse");
        ValidateMaterialTomlAsset(&asset).expect("flat should validate");
        assert!(
            asset.NodeById("color").is_some(),
            "NodeById should find color node"
        );
        assert_eq!(
            asset.NodeIds(),
            vec!["color".to_string(), "surface".to_string()],
            "node ids should preserve source order"
        );

        let topo = asset
            .TopologicalNodeIds()
            .expect("topological order should succeed");
        assert_eq!(
            topo,
            vec!["color".to_string(), "surface".to_string()],
            "dependencies should be emitted before dependents"
        );

        asset.Asset.Type = "not_material".to_string();
        assert!(matches!(
            ValidateMaterialTomlAsset(&asset),
            Err(MaterialTomlValidationError::InvalidAssetType { .. })
        ));
    }

    #[test]
    fn ValidateFailures() {
        let cases = [
            (
                "[asset]
type='material'
version=2
[material]
name='n'
output='n'
[[node]]
id='n'
kind='k'",
                "unsupported version",
            ),
            (
                "[asset]
type='material'
version=1
[material]
name=''
output='n'
[[node]]
id='n'
kind='k'",
                "empty material name",
            ),
            (
                "[asset]
type='material'
version=1
[material]
name='n'
output=''
[[node]]
id='n'
kind='k'",
                "empty material output",
            ),
            (
                "[asset]
type='material'
version=1
[material]
name='n'
output='missing'
[[node]]
id='n'
kind='k'",
                "unknown output node",
            ),
            (
                "[asset]
type='material'
version=1
[material]
name='n'
output='n'",
                "no nodes",
            ),
        ];
        for (src, label) in cases {
            let parsed = ParseMaterialToml("c.toml", src).expect("case should parse as toml");
            assert!(
                ValidateMaterialTomlAsset(&parsed).is_err(),
                "{label} should fail validation"
            );
        }

        let duplicate = ParseMaterialToml(
            "dup.toml",
            "[asset]
type='material'
version=1
[material]
name='n'
output='a'
[[node]]
id='a'
kind='x'
[[node]]
id='a'
kind='y'",
        )
        .expect("duplicate ids sample should parse");
        assert!(matches!(
            ValidateMaterialTomlAsset(&duplicate),
            Err(MaterialTomlValidationError::DuplicateNodeId { .. })
        ));

        let cycle = ParseMaterialToml(
            "cycle.toml",
            "[asset]
type='material'
version=1
[material]
name='n'
output='a'
[[node]]
id='a'
kind='mul'
[node.inputs]
x='b'
[[node]]
id='b'
kind='mul'
[node.inputs]
y='a'",
        )
        .expect("cycle sample should parse");
        assert!(matches!(
            ValidateMaterialTomlAsset(&cycle),
            Err(MaterialTomlValidationError::GraphCycleDetected)
        ));
    }

    #[test]
    fn SemanticValidationHappyPath() {
        let flat = ParseMaterialToml("flat.toml", FlatMaterialSource()).expect("flat parse");
        let semantics = ValidateMaterialTomlSemantics(&flat).expect("flat semantic validation");
        assert_eq!(semantics.OutputNodeId, "surface");
        assert_eq!(
            semantics.NodeTypes.get("color"),
            Some(&MaterialValueType::Float4)
        );
        assert_eq!(
            semantics.NodeTypes.get("surface"),
            Some(&MaterialValueType::Surface)
        );

        let tint = ParseMaterialToml("tint.toml", TextureTintSource()).expect("tint parse");
        let tint_semantics =
            ValidateMaterialTomlSemantics(&tint).expect("tint semantic validation");
        assert_eq!(
            tint_semantics.NodeTypes.get("base_color"),
            Some(&MaterialValueType::Float4)
        );
        assert_eq!(
            tint_semantics.TopologicalNodeIds.last().map(String::as_str),
            Some("surface"),
            "surface should be last reachable node in topological order"
        );
    }

    #[test]
    fn SemanticValidationRejectsInvalidKindsParamsAndTypes() {
        let unknown_reachable = ParseMaterialToml(
            "unknown.toml",
            "[asset]\ntype='material'\nversion=1\n[material]\nname='n'\noutput='surface'\n[[node]]\nid='x'\nkind='mystery'\n[[node]]\nid='surface'\nkind='standard_surface'\n[node.inputs]\nbase_color='x'",
        ).expect("unknown sample parse");
        assert!(matches!(
            ValidateMaterialTomlSemantics(&unknown_reachable),
            Err(MaterialSemanticError::UnsupportedNodeKind { .. })
        ));

        let unknown_unreachable = ParseMaterialToml(
            "unknown_unreach.toml",
            "[asset]\ntype='material'\nversion=1\n[material]\nname='n'\noutput='surface'\n[[node]]\nid='color'\nkind='constant_float4'\n[node.params]\nvalue=[1,1,1,1]\n[[node]]\nid='surface'\nkind='standard_surface'\n[node.inputs]\nbase_color='color'\n[[node]]\nid='x'\nkind='mystery'",
        ).expect("unknown-unreachable sample parse");
        ValidateMaterialTomlSemantics(&unknown_unreachable)
            .expect("unreachable unknown node should be ignored");

        let bad_constant = ParseMaterialToml(
            "bad_constant.toml",
            "[asset]\ntype='material'\nversion=1\n[material]\nname='n'\noutput='surface'\n[[node]]\nid='roughness'\nkind='constant_f32'\n[[node]]\nid='color'\nkind='constant_float4'\n[node.params]\nvalue=[1,1,1,1]\n[[node]]\nid='surface'\nkind='standard_surface'\n[node.inputs]\nbase_color='color'\nroughness='roughness'",
        ).expect("bad constant parse");
        assert!(matches!(
            ValidateMaterialTomlSemantics(&bad_constant),
            Err(MaterialSemanticError::MissingParam { .. })
        ));
    }

    #[test]
    fn MaterialSdslvCodegenHappyPathAndDeterminism() {
        let flat = ParseMaterialToml("flat.toml", FlatMaterialSource()).expect("flat parse");
        let a = GenerateMaterialSdslv(&flat).expect("flat codegen");
        let b = GenerateMaterialSdslv(&flat).expect("flat codegen repeat");
        assert_eq!(a, b, "material codegen must be deterministic");
        assert!(a.Source.contains("record MaterialSurface"));
        assert!(
            a.Source
                .contains("fn EvaluateMaterial() -> MaterialSurface")
        );
        assert!(a.Source.contains("float4(1.0, 0.0, 1.0, 1.0)"));
        assert!(a.Source.contains("surface.BaseColor = color;"));
        assert!(a.Source.contains("surface.Roughness = 0.5;"));
        assert!(a.Source.contains("surface.Metallic = 0.0;"));
    }

    #[test]
    fn MaterialSdslvCodegenTextureAndSanitizationAndDefaults() {
        let src = "[asset]
type='material'
version=1
[material]
name='San'
output='surface'
[[node]]
id='base-color'
kind='texture2d'
[node.params]
path='t.ppm'
[[node]]
id='base_color'
kind='constant_float4'
[node.params]
value=[0.5,0.5,0.5,1.0]
[[node]]
id='mixed'
kind='multiply'
[node.inputs]
a='base-color'
b='base_color'
[[node]]
id='surface'
kind='standard_surface'
[node.inputs]
base_color='mixed'";
        let asset = ParseMaterialToml("san.toml", src).expect("san parse");
        let out = GenerateMaterialSdslv(&asset).expect("san codegen");
        assert!(out.Source.contains("SampleTexture2D_base_color"));
        assert!(
            out.Source
                .contains("let mixed: float4 = base_color * base_color_1;")
        );
        assert!(out.Source.contains("surface.Roughness = 0.5;"));
        assert!(out.Source.contains("surface.Metallic = 0.0;"));
    }

    #[test]
    fn MaterialSdslvGeneratedSourceValidatesInSdslvParserValidator() {
        let tint = ParseMaterialToml("tint.toml", TextureTintSource()).expect("parse");
        let generated = GenerateMaterialSdslv(&tint).expect("codegen");
        crate::Engine::shader::sdslv::ValidateSource(&generated.Source)
            .expect("generated sdslv should parse + validate");
    }

    #[test]
    fn MaterialResourceRequirementsNoTextureCase() {
        let flat = ParseMaterialToml("flat.toml", FlatMaterialSource()).expect("flat parse");
        let requirements =
            CollectMaterialResourceRequirements(&flat).expect("flat requirements should build");
        assert_eq!(
            requirements.MaterialName, "FlatMagenta",
            "material name should be preserved in requirements"
        );
        assert!(
            requirements.Textures.is_empty(),
            "flat constant material should have no texture requirements"
        );
    }

    #[test]
    fn MaterialResourceRequirementsSingleTextureDefaultsAndNames() {
        let tint = ParseMaterialToml("tint.toml", TextureTintSource()).expect("tint parse");
        let requirements =
            CollectMaterialResourceRequirements(&tint).expect("requirements should build");
        assert_eq!(
            requirements.Textures.len(),
            1,
            "texture+tint fixture should produce one texture requirement"
        );
        let texture = &requirements.Textures[0];
        assert_eq!(texture.NodeId, "base_color_tex");
        assert_eq!(texture.SanitizedName, "base_color_tex");
        assert_eq!(texture.AssetPath, "textures/albedo.ppm");
        assert_eq!(texture.ColorSpace, MaterialTextureColorSpace::Srgb);
        assert_eq!(texture.BindingName, "tex_base_color_tex");
        assert_eq!(texture.Sampler.SamplerName, "samp_base_color_tex");
        let expected_plan = SamplerPlan::DefaultColor("samp_base_color_tex")
            .expect("expected default sampler plan should build");
        assert_eq!(
            texture.Sampler.Plan, expected_plan,
            "texture metadata should use default color sampler plan"
        );
    }

    #[test]
    fn MaterialResourceRequirementsMultipleTexturesDeterministicOrderAndSanitization() {
        let src = "[asset]
type='material'
version=1
[material]
name='MultiTex'
output='surface'
[[node]]
id='albedo-main'
kind='texture2d'
[node.params]
path='textures/a.ppm'
[[node]]
id='albedo_main'
kind='texture2d'
[node.params]
path='textures/b.ppm'
color_space='linear'
[[node]]
id='mixed'
kind='add'
[node.inputs]
a='albedo-main'
b='albedo_main'
[[node]]
id='surface'
kind='standard_surface'
[node.inputs]
base_color='mixed'";
        let asset = ParseMaterialToml("multi.toml", src).expect("multi parse");
        let requirements =
            CollectMaterialResourceRequirements(&asset).expect("requirements should build");
        assert_eq!(requirements.Textures.len(), 2);
        assert_eq!(
            requirements.Textures[0].NodeId, "albedo-main",
            "topological texture order should be deterministic"
        );
        assert_eq!(
            requirements.Textures[1].NodeId, "albedo_main",
            "topological texture order should be deterministic"
        );
        assert_eq!(requirements.Textures[0].SanitizedName, "albedo_main");
        assert_eq!(requirements.Textures[1].SanitizedName, "albedo_main_1");
        assert_eq!(requirements.Textures[0].BindingName, "tex_albedo_main");
        assert_eq!(requirements.Textures[1].BindingName, "tex_albedo_main_1");
        assert_eq!(
            requirements.Textures[1].ColorSpace,
            MaterialTextureColorSpace::Linear
        );
    }

    #[test]
    fn MaterialResourceRequirementsWrapsSemanticAndStructuralErrors() {
        let semantic_bad = ParseMaterialToml(
            "bad.toml",
            "[asset]\ntype='material'\nversion=1\n[material]\nname='n'\noutput='surface'\n[[node]]\nid='surface'\nkind='standard_surface'",
        )
        .expect("bad semantic sample parse");
        assert!(
            matches!(
                CollectMaterialResourceRequirements(&semantic_bad),
                Err(MaterialResourceRequirementError::Semantic(
                    MaterialSemanticError::MissingInput { .. }
                ))
            ),
            "semantic validation failures should be wrapped by requirement collection"
        );

        let mut structural_bad =
            ParseMaterialToml("flat.toml", FlatMaterialSource()).expect("flat parse");
        structural_bad.Asset.Type = "other".to_string();
        assert!(
            matches!(
                CollectMaterialResourceRequirements(&structural_bad),
                Err(MaterialResourceRequirementError::Structural(
                    MaterialTomlValidationError::InvalidAssetType { .. }
                ))
            ),
            "structural validation failures should be wrapped by requirement collection"
        );
    }

    #[test]
    fn MaterialResourceRequirementsAndCodegenShareSanitizedTextureName() {
        let src = "[asset]
type='material'
version=1
[material]
name='SharedNaming'
output='surface'
[[node]]
id='base-color'
kind='texture2d'
[node.params]
path='textures/base.ppm'
[[node]]
id='surface'
kind='standard_surface'
[node.inputs]
base_color='base-color'";
        let asset = ParseMaterialToml("shared.toml", src).expect("shared parse");
        let requirements =
            CollectMaterialResourceRequirements(&asset).expect("requirements should build");
        let generated = GenerateMaterialSdslv(&asset).expect("codegen should build");
        assert_eq!(requirements.Textures.len(), 1);
        let sanitized_name = &requirements.Textures[0].SanitizedName;
        assert_eq!(sanitized_name, "base_color");
        assert!(
            generated
                .Source
                .contains(&format!("SampleTexture2D_{sanitized_name}")),
            "texture placeholder naming should match metadata sanitizer naming"
        );
    }
}
