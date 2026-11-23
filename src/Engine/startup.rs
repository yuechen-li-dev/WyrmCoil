use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Diagnostic wall time for one startup call. The begin record also survives
/// a call that fails to return. Opt in with WYRMCOIL_VULKAN_STARTUP=1.
pub struct StartupStage {
    name: &'static str,
    start: Option<Instant>,
    id: u64,
}

impl StartupStage {
    pub fn Enabled() -> bool {
        static ENABLED: OnceLock<bool> = OnceLock::new();
        *ENABLED.get_or_init(|| std::env::var("WYRMCOIL_VULKAN_STARTUP").as_deref() == Ok("1"))
    }

    pub fn Begin(name: &'static str) -> Self {
        let start = Self::Enabled().then(Instant::now);
        let id = if start.is_some() {
            static NEXT: AtomicU64 = AtomicU64::new(1);
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            Self::Write(name, "begin", id, 0);
            id
        } else {
            0
        };
        Self { name, start, id }
    }

    pub fn Call<T>(name: &'static str, work: impl FnOnce() -> T) -> T {
        let _stage = Self::Begin(name);
        work()
    }

    pub fn Mark(name: &'static str, elapsed: Duration) {
        if Self::Enabled() {
            Self::Write(name, "mark", 0, elapsed.as_micros());
        }
    }

    fn Write(name: &str, event: &str, id: u64, micros: u128) {
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        eprintln!(
            "wyrmcoil_vk stage={name} event={event} unix_ms={} thread={:?} elapsed_us={micros} pid={} scope={id}",
            time.as_millis(),
            std::thread::current().id(),
            std::process::id(),
        );
    }
}

impl Drop for StartupStage {
    fn drop(&mut self) {
        if let Some(start) = self.start {
            Self::Write(self.name, "end", self.id, start.elapsed().as_micros());
        }
    }
}
