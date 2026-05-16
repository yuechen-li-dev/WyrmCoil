#![allow(non_snake_case)]

use super::artifact::{SdslvEntryPoint, SdslvShaderArtifact};
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DxcOptions {
    pub DxcPath: String,
    pub OutputSpirv: bool,
    pub ExtraArgs: Vec<String>,
}

impl Default for DxcOptions {
    fn default() -> Self {
        Self {
            DxcPath: "dxc".to_string(),
            OutputSpirv: true,
            ExtraArgs: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DxcCompileRequest {
    pub SourceName: String,
    pub Hlsl: String,
    pub EntryPoint: String,
    pub TargetProfile: String,
}

impl DxcCompileRequest {
    pub fn FromArtifactEntry(artifact: &SdslvShaderArtifact, entry: &SdslvEntryPoint) -> Self {
        Self {
            SourceName: artifact.SourceName.clone(),
            Hlsl: artifact.Hlsl.clone(),
            EntryPoint: entry.Name.clone(),
            TargetProfile: entry.TargetProfile.clone(),
        }
    }

    pub fn FromArtifactEntryName(
        artifact: &SdslvShaderArtifact,
        entry_name: &str,
    ) -> Result<Self, DxcError> {
        let entry = artifact
            .EntryPoints
            .iter()
            .find(|x| x.Name == entry_name)
            .ok_or_else(|| DxcError::EntryPointNotFound {
                EntryPoint: entry_name.to_string(),
                SourceName: artifact.SourceName.clone(),
            })?;
        Ok(Self::FromArtifactEntry(artifact, entry))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DxcCompileResult {
    pub Success: bool,
    pub EntryPoint: String,
    pub TargetProfile: String,
    pub Stdout: String,
    pub Stderr: String,
    pub OutputBytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DxcError {
    ToolUnavailable {
        Path: String,
    },
    IoError {
        Message: String,
    },
    CompileFailed {
        Result: DxcCompileResult,
    },
    OutputMissing {
        OutputPath: String,
        Result: DxcCompileResult,
    },
    EntryPointNotFound {
        EntryPoint: String,
        SourceName: String,
    },
}

pub fn FindDxc(options: &DxcOptions) -> bool {
    if !Path::new(&options.DxcPath).is_file() && options.DxcPath.contains(std::path::MAIN_SEPARATOR)
    {
        return false;
    }

    match Command::new(&options.DxcPath).arg("--version").output() {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

pub fn BuildDxcCommand(request: &DxcCompileRequest, options: &DxcOptions) -> Vec<String> {
    let mut args = Vec::new();
    args.push("-E".to_string());
    args.push(request.EntryPoint.clone());
    args.push("-T".to_string());
    args.push(request.TargetProfile.clone());
    if options.OutputSpirv {
        args.push("-spirv".to_string());
    }
    for arg in &options.ExtraArgs {
        args.push(arg.clone());
    }
    args
}

static DXC_TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn CompileHlslWithDxc(
    request: &DxcCompileRequest,
    options: &DxcOptions,
) -> Result<DxcCompileResult, DxcError> {
    if !FindDxc(options) {
        return Err(DxcError::ToolUnavailable {
            Path: options.DxcPath.clone(),
        });
    }

    let (input_path, output_path) = BuildTempPaths();
    fs::write(&input_path, &request.Hlsl).map_err(|e| DxcError::IoError {
        Message: format!("failed writing HLSL temp file '{}': {}", input_path, e),
    })?;

    let mut args = BuildDxcCommand(request, options);
    args.push(input_path.clone());
    args.push("-Fo".to_string());
    args.push(output_path.clone());

    let output = Command::new(&options.DxcPath)
        .args(&args)
        .output()
        .map_err(|e| DxcError::IoError {
            Message: format!("failed spawning DXC '{}': {}", options.DxcPath, e),
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    let mut result = DxcCompileResult {
        Success: output.status.success(),
        EntryPoint: request.EntryPoint.clone(),
        TargetProfile: request.TargetProfile.clone(),
        Stdout: stdout,
        Stderr: stderr,
        OutputBytes: Vec::new(),
    };

    if !output.status.success() {
        CleanupTempFiles(&input_path, &output_path);
        return Err(DxcError::CompileFailed { Result: result });
    }

    result.OutputBytes = fs::read(&output_path).map_err(|_| DxcError::OutputMissing {
        OutputPath: output_path.clone(),
        Result: result.clone(),
    })?;

    CleanupTempFiles(&input_path, &output_path);
    Ok(result)
}

pub fn CompileArtifactEntryWithDxc(
    artifact: &SdslvShaderArtifact,
    entry: &SdslvEntryPoint,
    options: &DxcOptions,
) -> Result<DxcCompileResult, DxcError> {
    let request = DxcCompileRequest::FromArtifactEntry(artifact, entry);
    CompileHlslWithDxc(&request, options)
}

fn BuildTempPaths() -> (String, String) {
    let mut base = env::temp_dir();
    let count = DXC_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    base.push(format!("wyrmcoil_sdslv_dxc_{}_{}_{}", pid, ts, count));
    let input = format!("{}.hlsl", base.display());
    let output = format!("{}.bin", base.display());
    (input, output)
}

fn CleanupTempFiles(input_path: &str, output_path: &str) {
    let _ = fs::remove_file(input_path);
    let _ = fs::remove_file(output_path);
}
