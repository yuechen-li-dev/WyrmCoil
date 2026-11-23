//! Derived SPIR-V from self-contained, validated SDSL-V artifacts.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::artifact::{SdslvEntryPoint, SdslvShaderArtifact, SdslvShaderStage};
use super::dxc::{BuildDxcCommand, CompileHlslWithDxc, DxcCompileRequest, DxcOptions};
use crate::Engine::startup::StartupStage;

const DIRECTORY: &str = "spirv-v1";
const MAGIC: &[u8; 8] = b"WYSPV\0\0\x01";
const HEADER_BYTES: usize = 80;
const MAX_ENTRY_BYTES: usize = 512 * 1024;
const MAX_ENTRIES: usize = 128;
const MAX_COMPILER_FILE_BYTES: u64 = 128 * 1024 * 1024;
// Bump when the SDSL-V artifact/compile contract changes. Exact generated HLSL
// also participates in the key, so unrelated compiler edits preserve hits.
const COMPILER_SCHEMA: &[u8] = b"wyrmcoil-sdslv-vdmir-hlsl-1";

#[derive(Debug, Clone, Copy)]
pub enum SpirvTarget {
    Vulkan11,
    Vulkan12,
    Vulkan13,
}

impl SpirvTarget {
    fn Flag(self) -> &'static str {
        match self {
            Self::Vulkan11 => "-fspv-target-env=vulkan1.1",
            Self::Vulkan12 => "-fspv-target-env=vulkan1.2",
            Self::Vulkan13 => "-fspv-target-env=vulkan1.3",
        }
    }

    fn MaxVersion(self) -> u32 {
        match self {
            Self::Vulkan11 => 0x0001_0300,
            Self::Vulkan12 => 0x0001_0500,
            Self::Vulkan13 => 0x0001_0600,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SpirvOptimization {
    Default,
    Disabled,
}

#[derive(Debug, Clone, Copy)]
pub struct SpirvConfig {
    pub Target: SpirvTarget,
    pub Optimization: SpirvOptimization,
    pub DebugInfo: bool,
}

impl SpirvConfig {
    pub const VULKAN_13: Self = Self {
        Target: SpirvTarget::Vulkan13,
        Optimization: SpirvOptimization::Default,
        DebugInfo: false,
    };
}

pub struct DxcSpirvCompiler {
    path: String,
    identity: Option<[u8; 32]>,
    cache: Option<PathBuf>,
}

impl DxcSpirvCompiler {
    /// Fingerprint the actual compiler once for a batch. Cache hits never
    /// start a DXC process, including to obtain its reported version.
    pub fn New(path: &str) -> Result<Self, String> {
        let path = ResolveCompiler(path)?;
        let cache = ShaderCacheDirectory();
        let identity = cache.as_ref().and_then(|_| {
            match StartupStage::Call("dxc_identity", || CompilerIdentity(&path)) {
                Ok(identity) => Some(identity),
                Err(error) => {
                    CacheTrace(&format!("event=bypass reason={error:?}"));
                    None
                }
            }
        });
        let path = path
            .into_os_string()
            .into_string()
            .map_err(|_| "DXC path is not Unicode".to_string())?;
        Ok(Self {
            path,
            identity,
            cache,
        })
    }

    pub fn Compile(
        &self,
        artifact: &SdslvShaderArtifact,
        entry: &SdslvEntryPoint,
        config: SpirvConfig,
    ) -> Result<Vec<u8>, String> {
        let request = DxcCompileRequest::FromArtifactEntry(artifact, entry);
        let mut extra_args = vec![config.Target.Flag().into()];
        if matches!(config.Optimization, SpirvOptimization::Disabled) {
            extra_args.push("-O0".into());
        }
        if config.DebugInfo {
            extra_args.push("-Zi".into());
        }
        let options = DxcOptions {
            DxcPath: self.path.clone(),
            OutputSpirv: true,
            ExtraArgs: extra_args,
        };
        let expected = ExpectedEntry {
            max_version: config.Target.MaxVersion(),
            model: match entry.Stage {
                SdslvShaderStage::Vertex => 0,
                SdslvShaderStage::Pixel => 4,
                SdslvShaderStage::Compute => 5,
            },
            name: &entry.Name,
        };
        let compile = || {
            CompileHlslWithDxc(&request, &options)
                .map(|output| output.OutputBytes)
                .map_err(|error| format!("{} / {}: {error:?}", artifact.SourceName, entry.Name))
        };
        // The parser rejects unresolved SDSL-V imports. Generated HLSL
        // only uses local SPIR-V guards around binding attributes.
        if let (Some(cache), Some(identity)) = (&self.cache, self.identity)
            && artifact.Hlsl.lines().all(|line| {
                let line = line.trim();
                !line.starts_with('#') || matches!(line, "#ifdef __spirv__" | "#endif")
            })
        {
            let key = Key(artifact, &request, &options, expected.model, &identity);
            return LoadOrCompile(cache, &key, &expected, compile);
        }
        let bytes = compile()?;
        CheckSpirv(&bytes, Some(&expected))?;
        Ok(bytes)
    }
}

fn ResolveCompiler(program: &str) -> Result<PathBuf, String> {
    let path = Path::new(program);
    if path.is_absolute() || path.components().count() > 1 {
        return path
            .canonicalize()
            .map_err(|error| format!("DXC unavailable at {program}: {error}"));
    }
    for directory in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        let mut candidate = directory.join(path);
        if cfg!(windows) && candidate.extension().is_none() {
            candidate.set_extension("exe");
        }
        if candidate.is_file() {
            return candidate.canonicalize().map_err(|error| error.to_string());
        }
    }
    Err(format!("DXC unavailable on PATH: {program}"))
}

fn CompilerIdentity(path: &Path) -> io::Result<[u8; 32]> {
    let mut hash = Sha256::new();
    HashFile(&mut hash, path)?;
    let directory = path.parent().unwrap();
    if cfg!(windows) {
        // The qualified SDK layout keeps the compiler DLL beside dxc.exe.
        // Bypass caching for layouts whose resolved compiler DLL is unknown.
        HashFile(&mut hash, &directory.join("dxcompiler.dll"))?;
    } else {
        return Err(io::Error::other(
            "DXC shared-library identity is unqualified",
        ));
    }
    Ok(hash.finalize().into())
}

fn HashFile(hash: &mut Sha256, path: &Path) -> io::Result<()> {
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    if size > MAX_COMPILER_FILE_BYTES {
        return Err(io::Error::other("compiler file exceeds fingerprint limit"));
    }
    hash.update(size.to_le_bytes());
    let mut buffer = [0; 64 * 1024];
    let mut remaining = size;
    while remaining > 0 {
        let count = remaining.min(buffer.len() as u64) as usize;
        file.read_exact(&mut buffer[..count])?;
        hash.update(&buffer[..count]);
        remaining -= count as u64;
    }
    if file.read(&mut buffer[..1])? != 0 {
        return Err(io::Error::other("compiler changed while fingerprinting"));
    }
    Ok(())
}

fn Key(
    artifact: &SdslvShaderArtifact,
    request: &DxcCompileRequest,
    options: &DxcOptions,
    model: u32,
    identity: &[u8; 32],
) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(MAGIC);
    hash.update(COMPILER_SCHEMA);
    hash.update(identity);
    hash.update(model.to_le_bytes());
    if let Some(source) = artifact.SourceDigest {
        hash.update([1]);
        hash.update(source);
    } else {
        hash.update([0]);
    }
    HashField(&mut hash, request.Hlsl.as_bytes());
    for arg in BuildDxcCommand(request, options) {
        HashField(&mut hash, arg.as_bytes());
    }
    hash.finalize().into()
}

fn HashField(hash: &mut Sha256, bytes: &[u8]) {
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}

/// A versioned subdirectory protects neighboring files when clearing the cache.
pub fn ShaderCacheDirectory() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("WYRMCOIL_SHADER_CACHE") {
        return (!path.is_empty()).then(|| PathBuf::from(path).join(DIRECTORY));
    }
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
    Some(
        local
            .join("WyrmCoil")
            .join("Cache")
            .join("shaders")
            .join(DIRECTORY),
    )
}

fn CacheTrace(message: &str) {
    if std::env::var("WYRMCOIL_SHADER_CACHE_TRACE").as_deref() == Ok("1") {
        eprintln!("wyrmcoil_shader_cache pid={} {message}", std::process::id());
    }
}

#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ShaderCacheActivity {
    pub Hits: u64,
    pub Misses: u64,
    pub Invalid: u64,
    pub AvoidedCompileUs: u64,
}

#[derive(Serialize)]
pub struct ShaderCacheStats {
    pub Entries: usize,
    pub Bytes: u64,
    pub InvalidEntries: usize,
    pub Activity: ShaderCacheActivity,
}

pub fn InspectShaderCache(directory: &Path, clear: bool) -> io::Result<ShaderCacheStats> {
    let _lock = Lock(directory)?;
    let mut stats = ShaderCacheStats {
        Entries: 0,
        Bytes: 0,
        InvalidEntries: 0,
        Activity: ReadActivity(directory),
    };
    for (path, _, size) in Entries(directory)? {
        stats.Entries += 1;
        stats.Bytes += size;
        if clear {
            fs::remove_file(path)?;
        } else if ReadRecord(&path, None).is_err() {
            stats.InvalidEntries += 1;
        }
    }
    if clear {
        for name in ["activity.json", "activity.pending", "artifact.pending"] {
            RemoveIfPresent(&directory.join(name))?;
        }
    }
    Ok(stats)
}

fn Lock(directory: &Path) -> io::Result<File> {
    fs::create_dir_all(directory)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join("publication.lock"))?;
    let start = Instant::now();
    loop {
        match lock.try_lock() {
            Ok(()) => return Ok(lock),
            Err(TryLockError::WouldBlock) if start.elapsed() < Duration::from_secs(10) => {
                std::thread::sleep(Duration::from_millis(15));
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn LoadOrCompile(
    directory: &Path,
    key: &[u8; 32],
    expected: &ExpectedEntry,
    compile: impl FnOnce() -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, String> {
    let lock = StartupStage::Call("shader_cache_lock", || Lock(directory));
    let Ok(_lock) = lock else {
        CacheTrace(&format!("event=bypass reason={lock:?}"));
        let bytes = compile()?;
        CheckSpirv(&bytes, Some(expected))?;
        return Ok(bytes);
    };
    let path = directory.join(EntryName(key));
    let mut activity = ReadActivity(directory);
    let record = StartupStage::Call("shader_cache_lookup", || {
        ReadRecord(&path, Some((key, expected)))
    });
    match record {
        Ok((bytes, compile_us)) => {
            activity.Hits = activity.Hits.saturating_add(1);
            activity.AvoidedCompileUs = activity.AvoidedCompileUs.saturating_add(compile_us);
            WriteActivity(directory, &activity);
            CacheTrace(&format!(
                "event=hit entry={} key={} avoided_us={compile_us}",
                expected.name,
                EntryName(key)
            ));
            return Ok(bytes);
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            activity.Invalid = activity.Invalid.saturating_add(1);
            CacheTrace(&format!(
                "event=invalid entry={} reason={error:?}",
                expected.name
            ));
        }
    }
    activity.Misses = activity.Misses.saturating_add(1);
    WriteActivity(directory, &activity);
    CacheTrace(&format!(
        "event=miss entry={} key={}",
        expected.name,
        EntryName(key)
    ));
    let start = Instant::now();
    let bytes = compile()?;
    let compile_us = start.elapsed().as_micros().min(u64::MAX as u128) as u64;
    StartupStage::Call("spirv_check", || CheckSpirv(&bytes, Some(expected)))?;
    if bytes.len() <= MAX_ENTRY_BYTES - HEADER_BYTES
        && let Err(error) = Publish(directory, &path, key, compile_us, &bytes)
    {
        CacheTrace(&format!(
            "event=write_failed entry={} reason={error:?}",
            expected.name
        ));
    }
    Ok(bytes)
}

fn EntryName(key: &[u8; 32]) -> String {
    use std::fmt::Write;
    let mut name = String::with_capacity(68);
    for byte in key {
        write!(name, "{byte:02x}").unwrap();
    }
    name.push_str(".spv");
    name
}

fn Entries(directory: &Path) -> io::Result<Vec<(PathBuf, SystemTime, u64)>> {
    let mut entries = Vec::new();
    for file in fs::read_dir(directory)? {
        let file = file?;
        let name = file.file_name();
        let Some(name) = name.to_str().and_then(|name| name.strip_suffix(".spv")) else {
            continue;
        };
        if name.len() == 64
            && name.bytes().all(|byte| byte.is_ascii_hexdigit())
            && file.file_type()?.is_file()
        {
            let meta = file.metadata()?;
            entries.push((
                file.path(),
                meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                meta.len(),
            ));
        }
    }
    Ok(entries)
}

fn Publish(
    directory: &Path,
    path: &Path,
    key: &[u8; 32],
    compile_us: u64,
    bytes: &[u8],
) -> io::Result<()> {
    let mut entries = Entries(directory)?;
    entries.retain(|(existing, _, _)| existing != path);
    if entries.len() >= MAX_ENTRIES {
        entries.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
        let remove = entries.len() + 1 - MAX_ENTRIES;
        for (old, _, _) in entries.into_iter().take(remove) {
            fs::remove_file(old)?;
        }
    }
    let mut record = Vec::with_capacity(HEADER_BYTES + bytes.len());
    record.extend_from_slice(MAGIC);
    record.extend_from_slice(key);
    record.extend_from_slice(&Sha256::digest(bytes));
    record.extend_from_slice(&compile_us.to_le_bytes());
    record.extend_from_slice(bytes);
    AtomicWrite(directory, "artifact.pending", path, &record)
}

fn ReadRecord(
    path: &Path,
    expected: Option<(&[u8; 32], &ExpectedEntry)>,
) -> io::Result<(Vec<u8>, u64)> {
    let mut record = ReadBounded(path, MAX_ENTRY_BYTES)?;
    if record.len() < HEADER_BYTES || &record[..8] != MAGIC {
        return Err(Invalid("shader cache header or version is invalid"));
    }
    let key: &[u8; 32] = record[8..40].try_into().unwrap();
    if path.file_name().and_then(|name| name.to_str()) != Some(&EntryName(key))
        || expected.is_some_and(|(wanted, _)| key != wanted)
    {
        return Err(Invalid("shader cache identity does not match"));
    }
    let payload = &record[HEADER_BYTES..];
    if Sha256::digest(payload)[..] != record[40..72] {
        return Err(Invalid("shader cache payload digest does not match"));
    }
    StartupStage::Call("spirv_check", || {
        CheckSpirv(payload, expected.map(|(_, entry)| entry))
    })
    .map_err(Invalid)?;
    let compile_us = u64::from_le_bytes(record[72..80].try_into().unwrap());
    Ok((record.split_off(HEADER_BYTES), compile_us))
}

fn AtomicWrite(directory: &Path, temp: &str, path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temp = directory.join(temp);
    RemoveIfPresent(&temp)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    let result = written.and_then(|()| fs::rename(&temp, path));
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

fn RemoveIfPresent(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

fn ReadBounded(path: &Path, max_size: usize) -> io::Result<Vec<u8>> {
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(Invalid("shader cache path is not a regular file"));
    }
    let mut bytes = Vec::new();
    File::open(path)?
        .take(max_size as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > max_size {
        return Err(Invalid("shader cache file exceeds size limit"));
    }
    Ok(bytes)
}

fn ReadActivity(directory: &Path) -> ShaderCacheActivity {
    ReadBounded(&directory.join("activity.json"), 1024)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn WriteActivity(directory: &Path, activity: &ShaderCacheActivity) {
    if let Ok(bytes) = serde_json::to_vec(activity)
        && let Err(error) = AtomicWrite(
            directory,
            "activity.pending",
            &directory.join("activity.json"),
            &bytes,
        )
    {
        CacheTrace(&format!("event=stats_write_failed reason={error:?}"));
    }
}

fn Invalid(message: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

struct ExpectedEntry<'a> {
    max_version: u32,
    model: u32,
    name: &'a str,
}

/// DXC validates on compilation. Reuse requires a matching digest, a complete
/// instruction stream and the expected SPIR-V version, entry name and model.
fn CheckSpirv(bytes: &[u8], entry: Option<&ExpectedEntry>) -> Result<(), &'static str> {
    if bytes.len() < 20 || bytes.len() % 4 != 0 {
        return Err("SPIR-V is truncated");
    }
    let word = |offset: usize| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    let version = word(4);
    let max_version = entry.map_or(0x0001_0600, |entry| entry.max_version);
    if word(0) != 0x0723_0203
        || !(0x10000..=max_version).contains(&version)
        || version & 0xff != 0
        || word(12) == 0
        || word(16) != 0
    {
        return Err("SPIR-V header or target version is invalid");
    }
    let mut offset = 20;
    let mut found = false;
    while offset < bytes.len() {
        let instruction = word(offset);
        let size = ((instruction >> 16) as usize) * 4;
        if size == 0 || size > bytes.len() - offset {
            return Err("SPIR-V instruction size is invalid");
        }
        if instruction & 0xffff == 15 {
            // OpEntryPoint: execution model, entry ID, null-terminated name.
            if size < 16 || word(offset + 8) == 0 || word(offset + 8) >= word(12) {
                return Err("SPIR-V entry point is invalid");
            }
            let name = &bytes[offset + 12..offset + size];
            let end = name
                .iter()
                .position(|byte| *byte == 0)
                .ok_or("SPIR-V entry name is truncated")?;
            found |= entry.is_none_or(|entry| {
                word(offset + 4) == entry.model && &name[..end] == entry.name.as_bytes()
            });
        }
        offset += size;
    }
    if !found {
        return Err("SPIR-V entry point or execution model does not match");
    }
    Ok(())
}

#[cfg(test)]
#[path = "spirv_cache_tests.rs"]
mod tests;
