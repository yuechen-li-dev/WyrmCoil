use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::compile::ValidateCompiled;
use super::{CompileScene, CompiledScene, SceneData, SceneDocument};

pub const ARTIFACT_PATH: &str = "target/wyrmcoil-scene.toml";
const VERSION: u32 = 1;
const MAX_BYTES: usize = 1024 * 1024;

#[derive(Serialize)]
struct ArtifactRef<'a> {
    version: u32,
    identity: &'a str,
    scene: &'a SceneData,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Artifact {
    version: u32,
    identity: String,
    scene: SceneData,
}

pub(super) fn ContentIdentity(data: &SceneData) -> Result<String, String> {
    let body = toml::to_string(data).map_err(|error| error.to_string())?;
    // Versioned FNV-1a content key for reproducibility and accidental corruption.
    let mut hash = 0xcbf29ce484222325u64;
    for byte in VERSION.to_le_bytes().into_iter().chain(body.bytes()) {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
    }
    Ok(format!("{hash:016x}"))
}

pub fn EncodeArtifact(scene: &CompiledScene) -> Result<String, String> {
    let text = toml::to_string(&ArtifactRef {
        version: VERSION,
        identity: scene.Identity(),
        scene: scene.Data(),
    })
    .map_err(|error| error.to_string())?;
    if text.len() > MAX_BYTES {
        return Err("scene artifact exceeds one MiB".into());
    }
    Ok(text)
}

pub fn DecodeArtifact(bytes: &[u8]) -> Result<CompiledScene, String> {
    if bytes.len() > MAX_BYTES {
        return Err("scene artifact exceeds one MiB".into());
    }
    let text =
        std::str::from_utf8(bytes).map_err(|error| format!("invalid scene UTF-8: {error}"))?;
    let artifact: Artifact = toml::from_str(text)
        .map_err(|error: toml::de::Error| format!("invalid scene artifact: {}", error.message()))?;
    if artifact.version != VERSION {
        return Err(format!(
            "unsupported scene artifact version {} (expected {VERSION})",
            artifact.version
        ));
    }
    let scene = ValidateCompiled(artifact.scene)?;
    if scene.Identity() != artifact.identity {
        return Err("scene artifact content identity mismatch".into());
    }
    Ok(scene)
}

pub fn ReadArtifact(path: &Path) -> Result<CompiledScene, String> {
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|file| file.take((MAX_BYTES + 1) as u64).read_to_end(&mut bytes))
        .map_err(|error| format!("{}: {error}", path.display()))?;
    DecodeArtifact(&bytes).map_err(|error| format!("{error} ({})", path.display()))
}

/// Runs validation and encoding before creating a temporary file alongside the destination.
pub fn BuildArtifact(document: &SceneDocument, path: &Path) -> Result<CompiledScene, String> {
    let scene = CompileScene(document)?;
    let artifact = EncodeArtifact(&scene)?;
    Publish(path, artifact.as_bytes()).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(scene)
}

fn Publish(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = path.with_extension(format!("tmp.{}.{nonce}", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    let result = written.and_then(|()| fs::rename(&temporary, path));
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}
