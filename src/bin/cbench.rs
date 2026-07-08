use std::env;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<_> = env::args_os().collect();
    if args.len() < 2 {
        eprintln!("Usage: cbench [-r runs] [-w warmup] <command> [args]");
        std::process::exit(1);
    }

    let mut runs = 100;
    let mut warmup = 3;
    let mut cmd_idx = 1;

    let mut i = 1;
    while i < args.len() {
        let arg = args[i].to_string_lossy();
        if arg == "-r" || arg == "--runs" {
            if i + 1 < args.len() {
                runs = args[i + 1]
                    .to_string_lossy()
                    .parse::<usize>()
                    .unwrap_or(100);
                i += 2;
            } else {
                break;
            }
        } else if arg == "-w" || arg == "--warmup" {
            if i + 1 < args.len() {
                warmup = args[i + 1].to_string_lossy().parse::<usize>().unwrap_or(3);
                i += 2;
            } else {
                break;
            }
        } else if arg.starts_with('-') {
            eprintln!("cbench: unknown option: {}", arg);
            std::process::exit(1);
        } else {
            cmd_idx = i;
            break;
        }
    }

    if cmd_idx >= args.len() {
        eprintln!("cbench: command missing");
        std::process::exit(1);
    }

    let cmd = &args[cmd_idx];
    let cmd_args = &args[cmd_idx + 1..];

    let cmd_str = args[cmd_idx..]
        .iter()
        .map(|s| s.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");

    println!("\x1b[38;2;137;180;250mBenchmarking:\x1b[0m {}", cmd_str);

    // Выполнение прогревочных запусков
    if warmup > 0 {
        println!("Performing {} warmup runs...", warmup);
        for _ in 0..warmup {
            let mut child = Command::new(cmd)
                .args(cmd_args)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let _ = child.wait();
        }
    }

    println!("Measuring {} runs...", runs);
    let mut times = Vec::with_capacity(runs);

    for _ in 0..runs {
        let start = Instant::now();
        let mut child = match Command::new(cmd)
            .args(cmd_args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                eprintln!("cbench: failed to execute: {}", e);
                std::process::exit(127);
            }
        };
        if child.wait().is_err() {
            std::process::exit(1);
        }
        times.push(start.elapsed());
    }

    // Сортируем времена для расчета медианы
    times.sort();

    let total_time: Duration = times.iter().sum();
    let total_secs = total_time.as_secs_f64();
    let mean = total_secs / (runs as f64);

    // Рассчитываем стандартное отклонение (Standard Deviation)
    let mut variance_sum = 0.0;
    for t in &times {
        let diff = t.as_secs_f64() - mean;
        variance_sum += diff * diff;
    }
    let variance = variance_sum / (runs as f64);
    let stddev = variance.sqrt();

    let median = times[runs / 2];
    let min = times[0];
    let max = times[runs - 1];

    println!("\n\x1b[38;2;166;227;161m\x1b[1mBenchmark results:\x1b[0m");
    println!("  Runs:        {}", runs);
    println!("  Average:     {:.3?}", Duration::from_secs_f64(mean));
    println!("  Median:      {:.3?}", median);
    println!("  StdDev (σ):  {:.3?}", Duration::from_secs_f64(stddev));
    println!("  Min..Max:    {:.3?} .. {:.3?}", min, max);
}
