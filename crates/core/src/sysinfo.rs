use std::sync::{Arc, Mutex};
use systemstat::{Platform, System};
use tokio::time::{Duration, interval};

#[derive(Clone, Default)]
pub struct SystemInfo {
    pub cpu_percent: f32,
    pub mem_used_gb: f32,
    pub mem_total_gb: f32,
    pub battery_percent: Option<f32>,
    pub battery_charging: bool,
}

impl SystemInfo {
    pub fn format(&self) -> String {
        let cpu = format!("CPU {:.0}%", self.cpu_percent);
        let mem = format!(
            "RAM {:.1}/{:.1}G",
            self.mem_used_gb, self.mem_total_gb
        );
        let bat = match self.battery_percent {
            Some(pct) => {
                let icon = if self.battery_charging { "+" } else { "" };
                format!("BAT {:.0}%{}", pct, icon)
            }
            None => String::new(),
        };
        let time = chrono::Local::now().format("TIME %Y-%m-%d %H:%M").to_string();

        let mut parts = vec![cpu, mem];
        if !bat.is_empty() {
            parts.push(bat);
        }
        parts.push(time);
        parts.join(" | ")
    }
}

pub fn spawn_sysinfo_poller() -> Arc<Mutex<SystemInfo>> {
    let info = Arc::new(Mutex::new(SystemInfo::default()));
    let info_clone = info.clone();

    tokio::spawn(async move {
        let mut tick = interval(Duration::from_secs(3));
        loop {
            tick.tick().await;
            let snapshot = tokio::task::spawn_blocking(sample_system_info)
                .await
                .unwrap_or_default();
            if let Ok(mut guard) = info_clone.lock() {
                *guard = snapshot;
            }
        }
    });

    info
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
