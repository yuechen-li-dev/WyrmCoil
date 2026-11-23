use super::*;
use crate::Engine::shader::sdslv::CompileCanonicalSourceToShaderArtifact;
use std::sync::Barrier;
use std::sync::atomic::{AtomicUsize, Ordering};

const ROOM: &str = include_str!("../../../../integrations/zed/wyrmcoil_view/src/tactical.sdslv");

struct TestCache(PathBuf);

impl TestCache {
    fn New() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let nonce = NEXT.fetch_add(1, Ordering::Relaxed);
        let stamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Self(std::env::temp_dir().join(format!(
            "wyrmcoil-m13-test-{}-{stamp}-{nonce}",
            std::process::id()
        )))
    }
}

impl Drop for TestCache {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn Expected() -> ExpectedEntry<'static> {
    ExpectedEntry {
        max_version: 0x0001_0600,
        model: 0,
        name: "main",
    }
}

fn Fixture(version: u32, model: u32) -> Vec<u8> {
    [
        0x0723_0203,
        version,
        0,
        2,
        0,
        (5 << 16) | 15,
        model,
        1,
        0x6e69_616d,
        0,
    ]
    .into_iter()
    .flat_map(u32::to_le_bytes)
    .collect()
}

#[test]
fn cache_hit_never_runs_the_compiler_and_clear_only_removes_cache_entries() {
    let cache = TestCache::New();
    let key = [7; 32];
    let expected = Expected();
    let binary = Fixture(0x0001_0600, 0);
    assert_eq!(
        LoadOrCompile(&cache.0, &key, &expected, || Ok(binary.clone())).unwrap(),
        binary
    );
    assert_eq!(
        LoadOrCompile(&cache.0, &key, &expected, || panic!(
            "cache hit called compiler"
        ))
        .unwrap(),
        binary
    );
    let stats = InspectShaderCache(&cache.0, false).unwrap();
    assert_eq!(
        (stats.Entries, stats.Activity.Misses, stats.Activity.Hits),
        (1, 1, 1)
    );
    fs::write(cache.0.join("authored.sdslv"), "source").unwrap();
    InspectShaderCache(&cache.0, true).unwrap();
    let stats = InspectShaderCache(&cache.0, false).unwrap();
    assert_eq!(
        (stats.Entries, stats.Activity.Misses, stats.Activity.Hits),
        (0, 0, 0)
    );
    assert!(cache.0.join("authored.sdslv").is_file());
}

#[test]
fn corrupt_wrong_target_and_interrupted_writes_rebuild_without_poisoning() {
    let cache = TestCache::New();
    let key = [91; 32];
    let expected = Expected();
    let binary = Fixture(0x0001_0600, 0);
    let path = cache.0.join(EntryName(&key));
    fs::create_dir(&cache.0).unwrap();
    fs::write(cache.0.join("artifact.pending"), b"incomplete").unwrap();
    assert!(LoadOrCompile(&cache.0, &key, &expected, || Err("compile error".into())).is_err());
    assert!(!path.exists());
    let rebuild = || LoadOrCompile(&cache.0, &key, &expected, || Ok(binary.clone())).unwrap();
    assert_eq!(rebuild(), binary);
    let good = fs::read(&path).unwrap();
    let mut bad_schema = good.clone();
    bad_schema[7] = 99;
    let mut bad_key = good.clone();
    bad_key[8] ^= 1;
    for invalid in [
        good[..35].to_vec(),
        good[..good.len() - 1].to_vec(),
        bad_schema,
        bad_key,
    ] {
        fs::write(&path, invalid).unwrap();
        assert_eq!(
            InspectShaderCache(&cache.0, false).unwrap().InvalidEntries,
            1
        );
        assert_eq!(rebuild(), binary);
    }
    // Even an otherwise intact payload must match the requested execution model and target.
    Publish(&cache.0, &path, &key, 0, &Fixture(0x0001_0600, 4)).unwrap();
    assert_eq!(rebuild(), binary);
    Publish(&cache.0, &path, &key, 0, &Fixture(0x0001_0600, 0)).unwrap();
    let old_target = ExpectedEntry {
        max_version: 0x0001_0500,
        ..expected
    };
    let older = Fixture(0x0001_0500, 0);
    assert_eq!(
        LoadOrCompile(&cache.0, &key, &old_target, || Ok(older.clone())).unwrap(),
        older
    );
    assert_eq!(
        InspectShaderCache(&cache.0, false)
            .unwrap()
            .Activity
            .Invalid,
        6
    );
}

#[test]
fn competing_compilation_requests_publish_once() {
    let cache = TestCache::New();
    let key = [9; 32];
    let arrivals = Barrier::new(4);
    let count = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                arrivals.wait();
                LoadOrCompile(&cache.0, &key, &Expected(), || {
                    count.fetch_add(1, Ordering::Relaxed);
                    Ok(Fixture(0x0001_0600, 0))
                })
                .unwrap();
            });
        }
    });
    assert_eq!(count.load(Ordering::Relaxed), 1);
    let stats = InspectShaderCache(&cache.0, false).unwrap();
    assert_eq!(
        (stats.Entries, stats.Activity.Misses, stats.Activity.Hits),
        (1, 1, 3)
    );
}

#[test]
fn cache_limit_evicts_owned_records_and_rejects_oversized_entries() {
    let cache = TestCache::New();
    let _lock = Lock(&cache.0).unwrap();
    let binary = Fixture(0x0001_0600, 0);
    for id in 0..=MAX_ENTRIES {
        let key = [id as u8; 32];
        Publish(&cache.0, &cache.0.join(EntryName(&key)), &key, 0, &binary).unwrap();
    }
    assert_eq!(Entries(&cache.0).unwrap().len(), MAX_ENTRIES);
    let oversized = cache.0.join("oversized");
    File::create(&oversized)
        .unwrap()
        .set_len(MAX_ENTRY_BYTES as u64 + 1)
        .unwrap();
    assert!(ReadBounded(&oversized, MAX_ENTRY_BYTES).is_err());
}

#[test]
fn cache_key_changes_selectively_and_unresolved_imports_cannot_enter_the_cache() {
    let cache = TestCache::New();
    let artifact = CompileCanonicalSourceToShaderArtifact("tactical.sdslv", ROOM).unwrap();
    let changed = CompileCanonicalSourceToShaderArtifact(
        "tactical.sdslv",
        &format!("{ROOM}\n// author edit"),
    )
    .unwrap();
    let entry = &artifact.EntryPoints[0];
    let request = DxcCompileRequest::FromArtifactEntry(&artifact, entry);
    let mut other_entry = request.clone();
    other_entry.EntryPoint.push_str("_2");
    let options = DxcOptions {
        ExtraArgs: vec!["-fspv-target-env=vulkan1.3".into()],
        ..Default::default()
    };
    let mut other_target = options.clone();
    other_target.ExtraArgs[0] = "-fspv-target-env=vulkan1.2".into();
    let mut other_flags = options.clone();
    other_flags.ExtraArgs.push("-O0".into());
    let identity = [42; 32];
    let key = Key(&artifact, &request, &options, 0, &identity);
    let keys = [
        key,
        Key(&changed, &request, &options, 0, &identity),
        Key(&artifact, &other_entry, &options, 0, &identity),
        Key(&artifact, &request, &options, 4, &identity),
        Key(&artifact, &request, &other_target, 0, &identity),
        Key(&artifact, &request, &other_flags, 0, &identity),
        Key(&artifact, &request, &options, 0, &[43; 32]),
    ];
    let calls = AtomicUsize::new(0);
    for key in keys.into_iter().chain(keys) {
        LoadOrCompile(&cache.0, &key, &Expected(), || {
            calls.fetch_add(1, Ordering::Relaxed);
            Ok(Fixture(0x0001_0600, 0))
        })
        .unwrap();
    }
    assert_eq!(calls.load(Ordering::Relaxed), keys.len());
    let unresolved = CompileCanonicalSourceToShaderArtifact(
        "imported.sdslv",
        &format!("import shared;\n{ROOM}"),
    );
    assert!(
        unresolved
            .unwrap_err()
            .iter()
            .any(|diagnostic| diagnostic.Message.contains("module resolution"))
    );
}

#[cfg(windows)]
#[test]
#[ignore = "requires the DXC Windows SDK and spirv-val"]
fn real_dxc_validates_both_stages_and_a_fresh_compiler_reuses_the_cache() {
    let cache = TestCache::New();
    let path = std::env::var("WYRMCOIL_DXC").unwrap_or_else(|_| "dxc".into());
    let mut producer = DxcSpirvCompiler::New(&path).unwrap();
    assert!(producer.identity.is_some());
    producer.cache = Some(cache.0.clone());
    let artifact = CompileCanonicalSourceToShaderArtifact("tactical.sdslv", ROOM).unwrap();
    let compiled: Vec<_> = artifact
        .EntryPoints
        .iter()
        .map(|entry| {
            producer
                .Compile(&artifact, entry, SpirvConfig::VULKAN_13)
                .unwrap()
        })
        .collect();
    let mut offline = DxcSpirvCompiler::New(&path).unwrap();
    offline.cache = Some(cache.0.clone());
    offline.path = cache
        .0
        .join("unavailable-dxc.exe")
        .to_string_lossy()
        .into_owned();
    for (entry, expected) in artifact.EntryPoints.iter().zip(compiled) {
        assert_eq!(
            offline
                .Compile(&artifact, entry, SpirvConfig::VULKAN_13)
                .unwrap(),
            expected
        );
        let spirv = cache.0.join("test-input.spv");
        fs::write(&spirv, expected).unwrap();
        let output = std::process::Command::new("spirv-val")
            .args(["--target-env", "vulkan1.3"])
            .arg(spirv)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let stats = InspectShaderCache(&cache.0, false).unwrap();
    assert_eq!(
        (stats.Entries, stats.Activity.Misses, stats.Activity.Hits),
        (2, 2, 2)
    );

    let skin_shader =
        include_str!("../../../../integrations/zed/wyrmcoil_view/src/tactical_skin.sdslv");
    let skin = CompileCanonicalSourceToShaderArtifact("tactical_skin.sdslv", skin_shader).unwrap();
    for entry in &skin.EntryPoints {
        producer
            .Compile(&skin, entry, SpirvConfig::VULKAN_13)
            .unwrap();
    }
    let modified_source =
        skin_shader.replace("return surface.Color;", "return surface.Color * 0.5;");
    let modified =
        CompileCanonicalSourceToShaderArtifact("tactical_skin.sdslv", &modified_source).unwrap();
    assert_ne!(skin.Hlsl, modified.Hlsl);
    for entry in &modified.EntryPoints {
        producer
            .Compile(&modified, entry, SpirvConfig::VULKAN_13)
            .unwrap();
    }
    for entry in &artifact.EntryPoints {
        offline
            .Compile(&artifact, entry, SpirvConfig::VULKAN_13)
            .unwrap();
    }
    for entry in &skin.EntryPoints {
        offline
            .Compile(&skin, entry, SpirvConfig::VULKAN_13)
            .unwrap();
    }
    let stats = InspectShaderCache(&cache.0, false).unwrap();
    assert_eq!(
        (stats.Entries, stats.Activity.Misses, stats.Activity.Hits),
        (6, 6, 6)
    );
}
