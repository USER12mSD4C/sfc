// src/bin/uniq.rs
use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;

const VERSION: &str = "uniq (sfc coreutils) 0.1.0";
const HELP: &str = "Usage: uniq [OPTION]... [INPUT [OUTPUT]]\n\
Filter adjacent matching lines from INPUT (or standard input), writing to OUTPUT (or standard output).\n\
\n\
  -c, --count           prefix lines by the number of occurrences\n\
  -d, --repeated        only print duplicate lines, one for each group\n\
  -D                    print all duplicate lines\n\
  -f, --skip-fields=N   avoid comparing the first N fields\n\
  -i, --ignore-case     ignore differences in case when comparing\n\
  -s, --skip-chars=N    avoid comparing the first N characters\n\
  -u, --unique          only print unique lines\n\
  -w, --check-chars=N   compare no more than N characters in lines\n\
      --help     display this help and exit\n\
      --version  output version information and exit";

struct Options {
    count: bool,
    repeated: bool,
    all_repeated: bool,
    unique_only: bool,
    ignore_case: bool,
    skip_fields: usize,
    skip_chars: usize,
    check_chars: usize,
    input: Option<PathBuf>,
    output: Option<PathBuf>,
}

fn parse_args() -> Result<Options, String> {
    let mut opts = Options {
        count: false,
        repeated: false,
        all_repeated: false,
        unique_only: false,
        ignore_case: false,
        skip_fields: 0,
        skip_chars: 0,
        check_chars: 0,
        input: None,
        output: None,
    };

    let mut args = env::args().skip(1);
    let mut end_of_opts = false;
    let mut positional = Vec::new();

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

            if arg.starts_with("-f") {
                let val = if arg.len() > 2 {
                    arg[2..].to_string()
                } else {
                    args.next().ok_or("-f requires arg")?
                };
                opts.skip_fields = val.parse().map_err(|_| "invalid number")?;
                continue;
            }
            if arg.starts_with("-s") {
                let val = if arg.len() > 2 {
                    arg[2..].to_string()
                } else {
                    args.next().ok_or("-s requires arg")?
                };
                opts.skip_chars = val.parse().map_err(|_| "invalid number")?;
                continue;
            }
            if arg.starts_with("-w") {
                let val = if arg.len() > 2 {
                    arg[2..].to_string()
                } else {
                    args.next().ok_or("-w requires arg")?
                };
                opts.check_chars = val.parse().map_err(|_| "invalid number")?;
                continue;
            }

            for c in arg.chars().skip(1) {
                match c {
                    'c' => opts.count = true,
                    'd' => opts.repeated = true,
                    'D' => opts.all_repeated = true,
                    'i' => opts.ignore_case = true,
                    'u' => opts.unique_only = true,
                    _ => return Err(format!("invalid option -- '{}'", c)),
                }
            }
        } else {
            positional.push(PathBuf::from(arg));
        }
    }

    if positional.len() > 2 {
        return Err("extra operand".into());
    }
    if positional.len() >= 1 {
        opts.input = Some(positional[0].clone());
    }
    if positional.len() == 2 {
        opts.output = Some(positional[1].clone());
    }

    Ok(opts)
}

fn get_key<'a>(line: &'a [u8], opts: &Options) -> &'a [u8] {
    let mut start = 0;
    let mut fields_skipped = 0;

    while fields_skipped < opts.skip_fields && start < line.len() {
        while start < line.len() && (line[start] == b' ' || line[start] == b'\t') {
            start += 1;
        }
        while start < line.len() && line[start] != b' ' && line[start] != b'\t' {
            start += 1;
        }
        fields_skipped += 1;
    }

    start += opts.skip_chars;
    if start > line.len() {
        start = line.len();
    }

    let mut end = line.len();
    if opts.check_chars > 0 && start + opts.check_chars < end {
        end = start + opts.check_chars;
    }

    &line[start..end]
}

fn lines_equal(a: &[u8], b: &[u8], opts: &Options) -> bool {
    let ka = get_key(a, opts);
    let kb = get_key(b, opts);
    if opts.ignore_case {
        if ka.len() != kb.len() {
            return false;
        }
        for i in 0..ka.len() {
            if ka[i].to_ascii_lowercase() != kb[i].to_ascii_lowercase() {
                return false;
            }
        }
        true
    } else {
        ka == kb
    }
}

fn main() -> ExitCode {
    let opts = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("uniq: {}", e);
            return ExitCode::from(1);
        }
    };

    let reader: Box<dyn BufRead> = match &opts.input {
        Some(p) if p != &PathBuf::from("-") => match File::open(p) {
            Ok(f) => Box::new(BufReader::new(f)),
            Err(e) => {
                eprintln!("uniq: cannot open '{}': {}", p.display(), e);
                return ExitCode::from(1);
            }
        },
        _ => Box::new(BufReader::new(io::stdin().lock())),
    };

    let writer: Box<dyn Write> = match &opts.output {
        Some(p) if p != &PathBuf::from("-") => match File::create(p) {
            Ok(f) => Box::new(BufWriter::new(f)),
            Err(e) => {
                eprintln!("uniq: cannot create '{}': {}", p.display(), e);
                return ExitCode::from(1);
            }
        },
        _ => Box::new(BufWriter::new(io::stdout().lock())),
    };

    let mut reader = reader;
    let mut writer = writer;

    let mut prev_line = Vec::new();
    let mut current_line = Vec::new();
    let mut count = 0;
    let mut is_first = true;

    loop {
        std::mem::swap(&mut prev_line, &mut current_line);
        current_line.clear();
        let n = reader.read_until(b'\n', &mut current_line).unwrap_or(0);

        if n == 0 {
            if !is_first {
                let should_print = (opts.unique_only && count == 1)
                    || (opts.repeated && count > 1)
                    || (!opts.unique_only && !opts.repeated && !opts.all_repeated);
                if should_print {
                    if opts.count {
                        write!(writer, "{:>7} ", count).unwrap();
                    }
                    writer.write_all(&prev_line).unwrap();
                }
            }
            break;
        }

        if is_first {
            count = 1;
            is_first = false;
        } else if lines_equal(&prev_line, &current_line, &opts) {
            count += 1;
        } else {
            let should_print = (opts.unique_only && count == 1)
                || (opts.repeated && count > 1)
                || (!opts.unique_only && !opts.repeated && !opts.all_repeated);
            if should_print {
                if opts.count {
                    write!(writer, "{:>7} ", count).unwrap();
                }
                writer.write_all(&prev_line).unwrap();
            }
            count = 1;
        }
    }
    ExitCode::from(0)
}
