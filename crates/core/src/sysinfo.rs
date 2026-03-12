use std::sync::Arc;
use parking_lot::Mutex;
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
