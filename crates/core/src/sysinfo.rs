use parking_lot::Mutex;
use serde::Serialize;
use std::sync::Arc;
use systemstat::{Platform, System};
use tokio::time::{Duration, interval};

#[derive(Clone, Default)]
struct SystemInfo {
    cpu_percent: f32,
    mem_used_gb: f32,
    mem_total_gb: f32,
    battery_percent: Option<f32>,
    battery_charging: bool,
}

impl SystemInfo {
    fn format(&self) -> String {
        let cpu = format!("CPU {:.0}%", self.cpu_percent);
        let mem = format!("RAM {:.1}/{:.1}G", self.mem_used_gb, self.mem_total_gb);
        let bat = match self.battery_percent {
            Some(pct) => {
                let icon = if self.battery_charging { "+" } else { "" };
                format!("BAT {:.0}%{}", pct, icon)
            }
            None => String::new(),
        };
        let time = chrono::Local::now()
            .format("TIME %Y-%m-%d %H:%M")
            .to_string();

        let mut parts = vec![cpu, mem];
        if !bat.is_empty() {
            parts.push(bat);
        }
        parts.push(time);
        parts.join(" | ")
    }
}

/// Structured system info snapshot for the desktop dashboard.
#[derive(Clone, Default, Serialize)]
pub struct SysInfoSnapshot {
    pub cpu_percent: f32,
    pub cpu_cores: Vec<CpuCoreInfo>,
    pub mem_used_gb: f32,
    pub mem_total_gb: f32,
    pub battery_percent: Option<f32>,
    pub battery_charging: bool,
    pub disk: Option<DiskInfo>,
    pub uptime_secs: Option<u64>,
    pub load_avg: Option<[f64; 3]>,
    pub hostname: String,
    pub os_name: String,
    pub timestamp: String,
}

#[derive(Clone, Serialize)]
pub struct CpuCoreInfo {
    pub core: u32,
    pub percent: f32,
}

#[derive(Clone, Serialize)]
pub struct DiskInfo {
    pub mount: String,
    pub total_gb: f32,
    pub used_gb: f32,
}

pub fn spawn_sysinfo_poller() -> Arc<Mutex<String>> {
    // Do a synchronous initial sample so the first frame isn't empty
    let initial = sample_system_info().format();
    let formatted = Arc::new(Mutex::new(initial));
    let formatted_clone = formatted.clone();

    tokio::spawn(async move {
        let mut tick = interval(Duration::from_secs(3));
        loop {
            tick.tick().await;
            let snapshot = tokio::task::spawn_blocking(|| sample_system_info().format())
                .await
                .unwrap_or_default();
            *formatted_clone.lock() = snapshot;
        }
    });

    formatted
}

/// Sample system info and return a formatted string. Blocking — suitable for
/// `spawn_blocking`. This is the public entry point for callers that manage
/// their own async runtime (e.g. the Tauri desktop app).
pub fn sample_formatted() -> String {
    sample_system_info().format()
}

/// Sample system info and return a structured snapshot. Blocking — suitable
/// for `spawn_blocking`.
pub fn sample_snapshot() -> SysInfoSnapshot {
    let sys = System::new();
    let mut snap = SysInfoSnapshot::default();

    // Per-core CPU — a single 200ms measurement gives us both per-core and aggregate
    if let Ok(cpus) = sys.cpu_load() {
        std::thread::sleep(Duration::from_millis(200));
        if let Ok(cpus) = cpus.done() {
            let cores: Vec<CpuCoreInfo> = cpus
                .iter()
                .enumerate()
                .map(|(i, c)| CpuCoreInfo {
                    core: i as u32,
                    percent: (1.0 - c.idle) * 100.0,
                })
                .collect();
            if !cores.is_empty() {
                snap.cpu_percent =
                    cores.iter().map(|c| c.percent).sum::<f32>() / cores.len() as f32;
            }
            snap.cpu_cores = cores;
        }
    }

    // Memory
    if let Ok(mem) = sys.memory() {
        let gib = 1024.0 * 1024.0 * 1024.0;
        let total = mem.total.as_u64() as f32 / gib;
        let free = mem.free.as_u64() as f32 / gib;
        snap.mem_total_gb = total;
        snap.mem_used_gb = total - free;
    }

    // Battery
    if let Ok(bat) = sys.battery_life() {
        snap.battery_percent = Some(bat.remaining_capacity * 100.0);
    }
    if let Ok(charging) = sys.on_ac_power() {
        snap.battery_charging = charging;
    }

    // Root disk only
    if let Ok(mounts) = sys.mounts() {
        let gib = 1024.0 * 1024.0 * 1024.0;
        if let Some(root) = mounts.iter().find(|m| m.fs_mounted_on == "/") {
            let total = root.total.as_u64() as f32 / gib;
            let free = root.avail.as_u64() as f32 / gib;
            snap.disk = Some(DiskInfo {
                mount: "/".to_string(),
                total_gb: total,
                used_gb: total - free,
            });
        }
    }

    // Uptime
    if let Ok(uptime) = sys.uptime() {
        snap.uptime_secs = Some(uptime.as_secs());
    }

    // Load average (Unix only)
    if let Ok(la) = sys.load_average() {
        snap.load_avg = Some([la.one as f64, la.five as f64, la.fifteen as f64]);
    }

    // Hostname — read from /etc/hostname (Linux) or fall back to env
    snap.hostname = std::fs::read_to_string("/etc/hostname")
        .map(|s| s.trim().to_string())
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_default();

    // OS name
    snap.os_name = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);

    // Timestamp
    snap.timestamp = chrono::Local::now()
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    snap
}

fn sample_system_info() -> SystemInfo {
    let sys = System::new();
    let mut info = SystemInfo::default();

    if let Ok(cpu) = sys.cpu_load_aggregate() {
        std::thread::sleep(Duration::from_millis(200));
        if let Ok(cpu) = cpu.done() {
            info.cpu_percent = (1.0 - cpu.idle) * 100.0;
        }
    }

    if let Ok(mem) = sys.memory() {
        let total = mem.total.as_u64() as f32 / (1024.0 * 1024.0 * 1024.0);
        let free = mem.free.as_u64() as f32 / (1024.0 * 1024.0 * 1024.0);
        info.mem_total_gb = total;
        info.mem_used_gb = total - free;
    }

    if let Ok(bat) = sys.battery_life() {
        info.battery_percent = Some(bat.remaining_capacity * 100.0);
    }
    if let Ok(charging) = sys.on_ac_power() {
        info.battery_charging = charging;
    }

    info
}
