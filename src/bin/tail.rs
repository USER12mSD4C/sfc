// src/bin/tail.rs
use std::collections::VecDeque;
use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::process::ExitCode;

const VERSION: &str = "tail (sfc coreutils) 0.1.0";
const HELP: &str = "Usage: tail [OPTION]... [FILE]...\n\
Print the last 10 lines of each FILE to standard output.\n\
\n\
  -c, --bytes=[+]NUM   output the last NUM bytes; or use -c +NUM to output starting with byte NUM\n\
  -n, --lines=[+]NUM   output the last NUM lines; or use -n +NUM to output starting with line NUM\n\
  -q, --quiet, --silent   never output headers giving file names\n\
  -v, --verbose        always output headers giving file names\n\
      --help     display this help and exit\n\
      --version  output version information and exit";

struct Options {
    lines: bool,
    count: usize,
    from_start: bool,
    quiet: bool,
    verbose: bool,
    files: Vec<PathBuf>,
}

fn parse_count(s: &str) -> Result<(usize, bool), String> {
    if s.starts_with('+') {
        let v = s[1..].parse::<usize>().map_err(|_| "invalid number")?;
        Ok((v, true))
    } else if s.starts_with('-') {
        let v = s[1..].parse::<usize>().map_err(|_| "invalid number")?;
        Ok((v, false))
    } else {
        let v = s.parse::<usize>().map_err(|_| "invalid number")?;
        Ok((v, false))
    }
}

fn parse_args() -> Result<Options, String> {
    let mut opts = Options {
        lines: true,
        count: 10,
        from_start: false,
        quiet: false,
        verbose: false,
        files: Vec::new(),
    };
    let mut args = env::args().skip(1);
    let mut end_of_opts = false;
    while let Some(arg) = args.next() {
        if !end_of_opts
            && arg.starts_with('-')
            && arg.len() > 1
            && arg[1..].chars().all(|c| c.is_ascii_digit())
        {
            opts.count = arg[1..].parse::<usize>().map_err(|_| "invalid number")?;
            opts.lines = true;
            opts.from_start = false;
            continue;
        }
        if !end_of_opts
            && arg.starts_with('+')
            && arg.len() > 1
            && arg[1..].chars().all(|c| c.is_ascii_digit())
        {
            opts.count = arg[1..].parse::<usize>().map_err(|_| "invalid number")?;
            opts.lines = true;
            opts.from_start = true;
            continue;
        }

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
            if arg == "-q" || arg == "--quiet" || arg == "--silent" {
                opts.quiet = true;
                continue;
            }
            if arg == "-v" || arg == "--verbose" {
                opts.verbose = true;
                continue;
            }
            if arg.starts_with("-n") {
                let val = if arg.len() > 2 {
                    arg[2..].to_string()
                } else {
                    args.next().ok_or("-n requires arg")?
                };
                let (c, f) = parse_count(&val)?;
                opts.count = c;
                opts.from_start = f;
                opts.lines = true;
                continue;
            }
            if arg.starts_with("-c") {
                let val = if arg.len() > 2 {
                    arg[2..].to_string()
                } else {
                    args.next().ok_or("-c requires arg")?
                };
                let (c, f) = parse_count(&val)?;
                opts.count = c;
                opts.from_start = f;
                opts.lines = false;
                continue;
            }
            for c in arg.chars().skip(1) {
                match c {
                    'q' => opts.quiet = true,
                    'v' => opts.verbose = true,
                    _ => return Err(format!("invalid option -- '{}'", c)),
                }
            }
        } else {
            opts.files.push(PathBuf::from(arg));
        }
    }
    Ok(opts)
}

fn tail_lines_stdin<R: Read, W: Write>(
    r: &mut R,
    w: &mut W,
    count: usize,
    from_start: bool,
) -> io::Result<()> {
    if from_start {
        let mut line = Vec::new();
        let mut r = BufReader::new(r);
        let mut skipped = 0;
        while skipped + 1 < count {
            line.clear();
            let n = r.read_until(b'\n', &mut line)?;
            if n == 0 {
                break;
            }
            skipped += 1;
        }
        io::copy(&mut r, w)?;
    } else {
        let mut buf = VecDeque::with_capacity(count);
        let mut r = BufReader::new(r);
        let mut line = Vec::new();
        loop {
            line.clear();
            let n = r.read_until(b'\n', &mut line)?;
            if n == 0 {
                break;
            }
            if buf.len() == count {
                buf.pop_front();
            }
            buf.push_back(std::mem::take(&mut line));
        }
        for l in buf {
            w.write_all(&l)?;
        }
    }
    Ok(())
}

fn tail_bytes_stdin<R: Read, W: Write>(
    r: &mut R,
    w: &mut W,
    count: usize,
    from_start: bool,
) -> io::Result<()> {
    if from_start {
        let mut r = BufReader::new(r);
        let mut buf = [0u8; 64 * 1024];
        let mut skipped = 0;
        while skipped < count {
            let to_read = std::cmp::min(count - skipped, buf.len());
            let n = r.read(&mut buf[..to_read])?;
            if n == 0 {
                break;
            }
            skipped += n;
        }
        io::copy(&mut r, w)?;
    } else {
        let mut buf = VecDeque::with_capacity(count);
        let mut tmp = [0u8; 64 * 1024];
        loop {
            let n = r.read(&mut tmp)?;
            if n == 0 {
                break;
            }
            for &b in &tmp[..n] {
                if buf.len() == count {
                    buf.pop_front();
                }
                buf.push_back(b);
            }
        }
        for b in buf {
            w.write_all(&[b])?;
        }
    }
    Ok(())
}

fn tail_file_lines<W: Write>(
    file: &mut File,
    w: &mut W,
    count: usize,
    from_start: bool,
) -> io::Result<()> {
    if from_start {
        let mut skipped = 0;
        let mut line = Vec::new();
        let mut r = BufReader::new(file);
        while skipped + 1 < count {
            line.clear();
            let n = r.read_until(b'\n', &mut line)?;
            if n == 0 {
                break;
            }
            skipped += 1;
        }
        io::copy(&mut r, w)?;
    } else {
        let size = file.metadata()?.len();
        if count == 0 {
            return Ok(());
        }
        if size == 0 {
            return Ok(());
        }

        let mut pos = size;
        let block_size = 8192;
        let mut buf = vec![0u8; block_size];
        let mut newlines_found = 0;

        loop {
            let read_size = std::cmp::min(pos, block_size as u64) as usize;
            if read_size == 0 {
                break;
            }
            pos -= read_size as u64;
            file.seek(SeekFrom::Start(pos))?;
            file.read_exact(&mut buf[..read_size])?;

            for i in (0..read_size).rev() {
                if buf[i] == b'\n' {
                    if newlines_found > 0 || (pos as usize + i) < (size as usize - 1) {
                        newlines_found += 1;
                        if newlines_found == count {
                            let start_pos = pos as usize + i + 1;
                            file.seek(SeekFrom::Start(start_pos as u64))?;
                            io::copy(file, w)?;
                            return Ok(());
                        }
                    }
                }
            }
        }
        file.seek(SeekFrom::Start(0))?;
        io::copy(file, w)?;
    }
    Ok(())
}

fn tail_file_bytes<W: Write>(
    file: &mut File,
    w: &mut W,
    count: usize,
    from_start: bool,
) -> io::Result<()> {
    if from_start {
        let mut skipped = 0;
        let mut buf = [0u8; 64 * 1024];
        while skipped < count {
            let to_read = std::cmp::min(count - skipped, buf.len());
            let n = file.read(&mut buf[..to_read])?;
            if n == 0 {
                break;
            }
            skipped += n;
        }
        io::copy(file, w)?;
    } else {
        let size = file.metadata()?.len();
        if count == 0 {
            return Ok(());
        }
        let start_pos = size.saturating_sub(count as u64);
        file.seek(SeekFrom::Start(start_pos))?;
        io::copy(file, w)?;
    }
    Ok(())
}

fn main() -> ExitCode {
    let opts = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("tail: {}", e);
            return ExitCode::from(1);
        }
    };

    let stdout = io::stdout();
    let mut w = BufWriter::with_capacity(64 * 1024, stdout.lock());
    let mut had_error = false;
    let files = if opts.files.is_empty() {
        vec![PathBuf::from("-")]
    } else {
        opts.files.clone()
    };
    let multiple = files.len() > 1;

    for (i, path) in files.iter().enumerate() {
        let name = if path == &PathBuf::from("-") {
            "standard input".to_string()
        } else {
            path.display().to_string()
        };

        let print_header = (multiple && !opts.quiet) || opts.verbose;
        if print_header {
            if i > 0 {
                writeln!(w).unwrap();
            }
            writeln!(w, "==> {} <==", name).unwrap();
        }

        let res = if path == &PathBuf::from("-") {
            let stdin = io::stdin();
            let mut r = stdin.lock();
            if opts.lines {
                tail_lines_stdin(&mut r, &mut w, opts.count, opts.from_start)
            } else {
                tail_bytes_stdin(&mut r, &mut w, opts.count, opts.from_start)
            }
        } else {
            match File::open(path) {
                Ok(mut f) => {
                    if opts.lines {
                        tail_file_lines(&mut f, &mut w, opts.count, opts.from_start)
                    } else {
                        tail_file_bytes(&mut f, &mut w, opts.count, opts.from_start)
                    }
                }
                Err(e) => {
                    eprintln!("tail: cannot open '{}' for reading: {}", name, e);
                    had_error = true;
                    Ok(())
                }
            }
        };
        if let Err(e) = res {
            eprintln!("tail: error reading '{}': {}", name, e);
            had_error = true;
        }
    }
    if had_error {
        ExitCode::from(1)
    } else {
        ExitCode::from(0)
    }
}
