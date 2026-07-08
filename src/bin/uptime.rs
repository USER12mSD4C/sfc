use std::fs;
use std::io;

fn main() -> io::Result<()> {
    let uptime_str = fs::read_to_string("/proc/uptime")?;
    let uptime_secs = uptime_str
        .split_whitespace()
        .next()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0) as u64;

    let h = uptime_secs / 3600;
    let m = (uptime_secs % 3600) / 60;

    let loadavg_str = fs::read_to_string("/proc/loadavg").unwrap_or_default();
    let loads: Vec<&str> = loadavg_str.split_whitespace().take(3).collect();
    let load_str = if loads.len() == 3 {
        format!("{}, {}, {}", loads[0], loads[1], loads[2])
    } else {
        "unknown".to_string()
    };

    println!("up {}h {}m, load average: {}", h, m, load_str);
    Ok(())
}
