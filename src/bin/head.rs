// src/bin/head.rs
use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::process::ExitCode;

const VERSION: &str = "head (sfc coreutils) 0.1.0";
const HELP: &str = "Usage: head [OPTION]... [FILE]...\n\
Print the first 10 lines of each FILE to standard output.\n\
\n\
  -c, --bytes=[-]NUM   print the first NUM bytes; with leading '-', print all but the last NUM bytes\n\
  -n, --lines=[-]NUM   print the first NUM lines; with leading '-', print all but the last NUM lines\n\
  -q, --quiet, --silent   never print headers giving file names\n\
  -v, --verbose        always print headers giving file names\n\
      --help     display this help and exit\n\
      --version  output version information and exit";

struct Options {
    lines: bool,
    count: usize,
    exclude_end: bool,
    quiet: bool,
    verbose: bool,
    files: Vec<PathBuf>,
}

fn parse_count(s: &str) -> Result<(usize, bool), String> {
    if s.starts_with('-') {
        let v = s[1..].parse::<usize>().map_err(|_| "invalid number")?;
        Ok((v, true))
    } else if s.starts_with('+') {
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
        exclude_end: false,
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
                let (c, e) = parse_count(&val)?;
                opts.count = c;
                opts.exclude_end = e;
                opts.lines = true;
                continue;
            }
            if arg.starts_with("-c") {
                let val = if arg.len() > 2 {
                    arg[2..].to_string()
                } else {
                    args.next().ok_or("-c requires arg")?
                };
                let (c, e) = parse_count(&val)?;
                opts.count = c;
                opts.exclude_end = e;
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

fn head_lines<R: BufRead, W: Write>(r: &mut R, w: &mut W, count: usize) -> io::Result<()> {
    let mut line = Vec::with_capacity(128);
    for _ in 0..count {
        line.clear();
        let n = r.read_until(b'\n', &mut line)?;
        if n == 0 {
            break;
        }
        w.write_all(&line)?;
    }
    Ok(())
}

fn head_bytes<R: Read, W: Write>(r: &mut R, w: &mut W, count: usize) -> io::Result<()> {
    let mut buf = [0u8; 64 * 1024];
    let mut remaining = count;
    while remaining > 0 {
        let to_read = std::cmp::min(remaining, buf.len());
        let n = r.read(&mut buf[..to_read])?;
        if n == 0 {
            break;
        }
        w.write_all(&buf[..n])?;
        remaining -= n;
    }
    Ok(())
}

fn head_file_lines<W: Write>(
    file: &mut File,
    w: &mut W,
    count: usize,
    exclude_end: bool,
) -> io::Result<()> {
    if !exclude_end {
        let mut r = BufReader::new(file);
        return head_lines(&mut r, w, count);
    }
    let size = file.metadata()?.len();
    if count == 0 {
        file.seek(SeekFrom::Start(0))?;
        io::copy(file, w)?;
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
                        let end_pos = pos as usize + i + 1;
                        file.seek(SeekFrom::Start(0))?;
                        let mut remaining = end_pos;
                        let mut tmp_buf = [0u8; 64 * 1024];
                        while remaining > 0 {
                            let to_read = std::cmp::min(remaining, tmp_buf.len());
                            let n = file.read(&mut tmp_buf[..to_read])?;
                            if n == 0 {
                                break;
                            }
                            w.write_all(&tmp_buf[..n])?;
                            remaining -= n;
                        }
                        return Ok(());
                    }
                }
            }
        }
    }
    Ok(())
}

fn head_file_bytes<W: Write>(
    file: &mut File,
    w: &mut W,
    count: usize,
    exclude_end: bool,
) -> io::Result<()> {
    if !exclude_end {
        return head_bytes(file, w, count);
    }
    let size = file.metadata()?.len() as usize;
    let keep = size.saturating_sub(count);
    file.seek(SeekFrom::Start(0))?;
    let mut buf = [0u8; 64 * 1024];
    let mut read_so_far = 0;
    while read_so_far < keep {
        let to_read = std::cmp::min(keep - read_so_far, buf.len());
        let n = file.read(&mut buf[..to_read])?;
        if n == 0 {
            break;
        }
        w.write_all(&buf[..n])?;
        read_so_far += n;
    }
    Ok(())
}

fn main() -> ExitCode {
    let opts = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("head: {}", e);
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
                head_lines(&mut r, &mut w, opts.count)
            } else {
                head_bytes(&mut r, &mut w, opts.count)
            }
        } else {
            match File::open(path) {
                Ok(mut f) => {
                    if opts.lines {
                        head_file_lines(&mut f, &mut w, opts.count, opts.exclude_end)
                    } else {
                        head_file_bytes(&mut f, &mut w, opts.count, opts.exclude_end)
                    }
                }
                Err(e) => {
                    eprintln!("head: cannot open '{}' for reading: {}", name, e);
                    had_error = true;
                    Ok(())
                }
            }
        };
        if let Err(e) = res {
            eprintln!("head: error reading '{}': {}", name, e);
            had_error = true;
        }
    }
    if had_error {
        ExitCode::from(1)
    } else {
        ExitCode::from(0)
    }
}
