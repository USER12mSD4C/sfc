// src/bin/wc.rs
use std::env;
use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

const VERSION: &str = "wc (sfc coreutils) 0.1.0";
const HELP: &str = "Usage: wc [OPTION]... [FILE]...\n\
Print newline, word, and byte counts for each FILE, and a total line if more than one FILE is specified.\n\
\n\
  -c, --bytes            print the byte counts\n\
  -m, --chars            print the character counts\n\
  -l, --lines            print the newline counts\n\
  -L, --max-line-length  print the maximum display width\n\
  -w, --words            print the word counts\n\
      --help     display this help and exit\n\
      --version  output version information and exit";

struct Options {
    bytes: bool,
    chars: bool,
    lines: bool,
    words: bool,
    max_line_len: bool,
    files: Vec<PathBuf>,
}

fn parse_args() -> Result<Options, String> {
    let mut opts = Options {
        bytes: false,
        chars: false,
        lines: false,
        words: false,
        max_line_len: false,
        files: Vec::new(),
    };
    let mut args = env::args().skip(1);
    let mut end_of_opts = false;
    while let Some(arg) = args.next() {
        if !end_of_opts && arg.starts_with('-') && arg.len() > 1 {
            if arg == "--" {
                end_of_opts = true;
                continue;
            }
            if arg == "--help" {
                println!("{}", HELP);
                std::process::exit(0);
            }
            if arg == "--version" {
                println!("{}", VERSION);
                std::process::exit(0);
            }
            for c in arg.chars().skip(1) {
                match c {
                    'c' => opts.bytes = true,
                    'm' => opts.chars = true,
                    'l' => opts.lines = true,
                    'L' => opts.max_line_len = true,
                    'w' => opts.words = true,
                    _ => return Err(format!("invalid option -- '{}'", c)),
                }
            }
        } else {
            opts.files.push(PathBuf::from(arg));
        }
    }
    if !opts.bytes && !opts.chars && !opts.lines && !opts.words && !opts.max_line_len {
        opts.lines = true;
        opts.words = true;
        opts.bytes = true;
    }
    Ok(opts)
}

#[inline]
fn count_chars_in_slice(slice: &[u8]) -> usize {
    std::str::from_utf8(slice)
        .map(|s| s.chars().count())
        .unwrap_or_else(|_| {
            slice
                .iter()
                .filter(|&&b| b < 128 || (b & 0xC0) != 0x80)
                .count()
        })
}

struct Stats {
    lines: usize,
    words: usize,
    bytes: usize,
    chars: usize,
    max_line_len: usize,
}

fn process<R: Read>(mut reader: R, opts: &Options) -> io::Result<Stats> {
    let mut stats = Stats {
        lines: 0,
        words: 0,
        bytes: 0,
        chars: 0,
        max_line_len: 0,
    };
    let mut buf = [0u8; 128 * 1024];
    let mut in_word = false;
    let mut current_line_len = 0;

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        let slice = &buf[..n];
        stats.bytes += n;

        if opts.chars {
            stats.chars += count_chars_in_slice(slice);
        }

        if opts.lines || opts.max_line_len || opts.words {
            for &b in slice {
                if opts.lines || opts.max_line_len {
                    if b == b'\n' {
                        stats.lines += 1;
                        if opts.max_line_len && current_line_len > stats.max_line_len {
                            stats.max_line_len = current_line_len;
                        }
                        current_line_len = 0;
                    } else if b != b'\r' {
                        current_line_len += 1;
                    }
                }
                if opts.words {
                    if b.is_ascii_whitespace() {
                        in_word = false;
                    } else if !in_word {
                        in_word = true;
                        stats.words += 1;
                    }
                }
            }
        }
    }
    if opts.max_line_len && current_line_len > stats.max_line_len {
        stats.max_line_len = current_line_len;
    }
    Ok(stats)
}

fn print_stats<W: Write>(
    w: &mut W,
    stats: &Stats,
    name: &str,
    opts: &Options,
    width: usize,
) -> io::Result<()> {
    let mut parts = Vec::new();
    if opts.lines {
        parts.push(format!("{:>width$}", stats.lines));
    }
    if opts.words {
        parts.push(format!("{:>width$}", stats.words));
    }
    if opts.chars {
        parts.push(format!("{:>width$}", stats.chars));
    }
    if opts.bytes {
        parts.push(format!("{:>width$}", stats.bytes));
    }
    if opts.max_line_len {
        parts.push(format!("{:>width$}", stats.max_line_len));
    }
    if !name.is_empty() {
        parts.push(name.to_string());
    }
    writeln!(w, "{}", parts.join(" "))
}

fn main() -> ExitCode {
    let opts = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("wc: {}", e);
            return ExitCode::from(1);
        }
    };

    let stdout = io::stdout();
    let mut w = BufWriter::with_capacity(64 * 1024, stdout.lock());

    let mut width = 7;
    if !opts.files.is_empty() {
        let mut total = 0;
        for p in &opts.files {
            if let Ok(m) = fs::symlink_metadata(p) {
                if m.is_file() {
                    total += m.len();
                }
            }
        }
        if total > 0 {
            width = total.to_string().len().max(1);
        }
    }

    let mut total_stats = Stats {
        lines: 0,
        words: 0,
        bytes: 0,
        chars: 0,
        max_line_len: 0,
    };
    let mut had_error = false;

    if opts.files.is_empty() {
        let stdin = io::stdin();
        if let Ok(stats) = process(stdin.lock(), &opts) {
            let _ = print_stats(&mut w, &stats, "", &opts, width);
        } else {
            had_error = true;
        }
    } else {
        for path in &opts.files {
            match File::open(path) {
                Ok(f) => {
                    if let Ok(stats) = process(f, &opts) {
                        let _ = print_stats(&mut w, &stats, &path.to_string_lossy(), &opts, width);
                        total_stats.lines += stats.lines;
                        total_stats.words += stats.words;
                        total_stats.bytes += stats.bytes;
                        total_stats.chars += stats.chars;
                        if stats.max_line_len > total_stats.max_line_len {
                            total_stats.max_line_len = stats.max_line_len;
                        }
                    } else {
                        had_error = true;
                    }
                }
                Err(e) => {
                    eprintln!("wc: {}: {}", path.display(), e);
                    had_error = true;
                }
            }
        }
        if opts.files.len() > 1 {
            let _ = print_stats(&mut w, &total_stats, "total", &opts, width);
        }
    }
    if had_error {
        ExitCode::from(1)
    } else {
        ExitCode::from(0)
    }
}
