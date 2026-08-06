// src/bin/sort.rs
use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;

const VERSION: &str = "sort (sfc coreutils) 0.1.0";
const HELP: &str = "Usage: sort [OPTION]... [FILE]...\n\
Write sorted concatenation of all FILE(s) to standard output.\n\
\n\
  -n, --numeric-sort       compare according to string numerical value\n\
  -g, --general-numeric-sort  compare according to general numerical value\n\
  -h, --human-numeric-sort  compare human readable numbers (e.g., 2K 1G)\n\
  -r, --reverse              reverse the result of comparisons\n\
  -u, --unique               output only the first of an equal run\n\
  -z, --zero-terminated      line delimiter is NUL, not newline\n\
  -o, --output=FILE          write result to FILE instead of standard output\n\
      --help     display this help and exit\n\
      --version  output version information and exit";

#[derive(PartialEq)]
enum Mode {
    Default,
    Numeric,
    GeneralNumeric,
    Human,
}

struct Options {
    mode: Mode,
    reverse: bool,
    unique: bool,
    zero_terminated: bool,
    output: Option<PathBuf>,
    files: Vec<PathBuf>,
}

fn parse_args() -> Result<Options, String> {
    let mut opts = Options {
        mode: Mode::Default,
        reverse: false,
        unique: false,
        zero_terminated: false,
        output: None,
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
            if arg.starts_with("-o") {
                let val = if arg.len() > 2 {
                    arg[2..].to_string()
                } else {
                    args.next().ok_or("-o requires arg")?
                };
                opts.output = Some(PathBuf::from(val));
                continue;
            }
            if arg == "--output" {
                opts.output = Some(PathBuf::from(args.next().ok_or("--output requires arg")?));
                continue;
            }
            for c in arg.chars().skip(1) {
                match c {
                    'n' => opts.mode = Mode::Numeric,
                    'g' => opts.mode = Mode::GeneralNumeric,
                    'h' => opts.mode = Mode::Human,
                    'r' => opts.reverse = true,
                    'u' => opts.unique = true,
                    'z' => opts.zero_terminated = true,
                    _ => return Err(format!("invalid option -- '{}'", c)),
                }
            }
        } else {
            opts.files.push(PathBuf::from(arg));
        }
    }
    Ok(opts)
}

fn parse_numeric(s: &[u8]) -> f64 {
    let mut start = 0;
    while start < s.len() && (s[start] == b' ' || s[start] == b'\t') {
        start += 1;
    }
    let mut end = start;
    if end < s.len() && (s[end] == b'-' || s[end] == b'+') {
        end += 1;
    }
    while end < s.len() && s[end].is_ascii_digit() {
        end += 1;
    }
    if end < s.len() && s[end] == b'.' {
        end += 1;
        while end < s.len() && s[end].is_ascii_digit() {
            end += 1;
        }
    }
    if end > start {
        std::str::from_utf8(&s[start..end])
            .unwrap_or("0")
            .parse()
            .unwrap_or(0.0)
    } else {
        0.0
    }
}

fn parse_human(s: &[u8]) -> f64 {
    let val = parse_numeric(s);
    let mut end = 0;
    while end < s.len()
        && (s[end].is_ascii_digit()
            || s[end] == b'.'
            || s[end] == b'-'
            || s[end] == b'+'
            || s[end] == b' '
            || s[end] == b'\t')
    {
        end += 1;
    }
    if end < s.len() {
        match s[end].to_ascii_uppercase() {
            b'K' => val * 1024.0,
            b'M' => val * 1024.0 * 1024.0,
            b'G' => val * 1024.0 * 1024.0 * 1024.0,
            b'T' => val * 1024.0 * 1024.0 * 1024.0 * 1024.0,
            _ => val,
        }
    } else {
        val
    }
}

fn main() -> ExitCode {
    let opts = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("sort: {}", e);
            return ExitCode::from(1);
        }
    };

    let mut lines: Vec<Vec<u8>> = Vec::new();
    let delim = if opts.zero_terminated { b'\0' } else { b'\n' };

    let read_input = |lines: &mut Vec<Vec<u8>>, path: Option<&PathBuf>| -> io::Result<()> {
        let mut reader: Box<dyn BufRead> = match path {
            Some(p) if p != &PathBuf::from("-") => Box::new(BufReader::new(File::open(p)?)),
            _ => Box::new(BufReader::new(io::stdin().lock())),
        };
        loop {
            let mut line = Vec::new();
            let n = reader.read_until(delim, &mut line)?;
            if n == 0 {
                break;
            }
            lines.push(line);
        }
        Ok(())
    };

    if opts.files.is_empty() {
        if let Err(e) = read_input(&mut lines, None) {
            eprintln!("sort: read error: {}", e);
            return ExitCode::from(1);
        }
    } else {
        for f in &opts.files {
            if let Err(e) = read_input(&mut lines, Some(f)) {
                eprintln!("sort: cannot read '{}': {}", f.display(), e);
                return ExitCode::from(1);
            }
        }
    }

    lines.sort_unstable_by(|a, b| {
        let cmp = match opts.mode {
            Mode::Default => a.cmp(b),
            Mode::Numeric | Mode::GeneralNumeric => parse_numeric(a)
                .partial_cmp(&parse_numeric(b))
                .unwrap_or(std::cmp::Ordering::Equal),
            Mode::Human => parse_human(a)
                .partial_cmp(&parse_human(b))
                .unwrap_or(std::cmp::Ordering::Equal),
        };
        if opts.reverse {
            cmp.reverse()
        } else {
            cmp
        }
    });

    let mut writer: Box<dyn Write> = match &opts.output {
        Some(p) if p != &PathBuf::from("-") => match File::create(p) {
            Ok(f) => Box::new(BufWriter::new(f)),
            Err(e) => {
                eprintln!("sort: cannot create '{}': {}", p.display(), e);
                return ExitCode::from(1);
            }
        },
        _ => Box::new(BufWriter::new(io::stdout().lock())),
    };

    if opts.unique {
        let mut last_printed: Option<&[u8]> = None;
        for line in &lines {
            let should_print = match last_printed {
                None => true,
                Some(prev) => {
                    let eq = match opts.mode {
                        Mode::Default => prev == line.as_slice(),
                        Mode::Numeric | Mode::GeneralNumeric => {
                            parse_numeric(prev) == parse_numeric(line)
                        }
                        Mode::Human => parse_human(prev) == parse_human(line),
                    };
                    !eq
                }
            };
            if should_print {
                writer.write_all(line).unwrap();
                last_printed = Some(line);
            }
        }
    } else {
        for line in &lines {
            writer.write_all(line).unwrap();
        }
    }
    ExitCode::from(0)
}
