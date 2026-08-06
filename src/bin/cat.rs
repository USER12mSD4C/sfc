// src/bin/cat.rs
use std::env;
use std::fs::File;
use std::io::{self, stdin, stdout, BufRead, BufReader, BufWriter, Read, Write};
use std::process::ExitCode;

const VERSION: &str = "cat (sfc coreutils) 0.1.0";
const HELP: &str = "Usage: cat [OPTION]... [FILE]...\n\
Concatenate FILE(s) to standard output.\n\
\n\
With no FILE, or when FILE is -, read standard input.\n\
\n\
  -A, --show-all           equivalent to -vET\n\
  -b, --number-nonblank    number nonempty output lines, overrides -n\n\
  -e                       equivalent to -vE\n\
  -E, --show-ends          display $ at end of each line\n\
  -n, --number             number all output lines\n\
  -s, --squeeze-blank      suppress repeated empty output lines\n\
  -t                       equivalent to -vT\n\
  -T, --show-tabs          display TAB characters as ^I\n\
  -u                       (ignored)\n\
  -v, --show-nonprinting   use ^ and M- notation, except for LFD and TAB\n\
      --help     display this help and exit\n\
      --version  output version information and exit";

struct Options {
    number_all: bool,
    number_nonblank: bool,
    squeeze_blank: bool,
    show_ends: bool,
    show_tabs: bool,
    show_nonprinting: bool,
    files: Vec<String>,
}

fn parse_args() -> Result<Options, String> {
    let mut opts = Options {
        number_all: false,
        number_nonblank: false,
        squeeze_blank: false,
        show_ends: false,
        show_tabs: false,
        show_nonprinting: false,
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

            match arg.as_str() {
                "--show-all" => {
                    opts.show_nonprinting = true;
                    opts.show_ends = true;
                    opts.show_tabs = true;
                    continue;
                }
                "--number-nonblank" => {
                    opts.number_nonblank = true;
                    continue;
                }
                "--show-ends" => {
                    opts.show_ends = true;
                    continue;
                }
                "--number" => {
                    opts.number_all = true;
                    continue;
                }
                "--squeeze-blank" => {
                    opts.squeeze_blank = true;
                    continue;
                }
                "--show-tabs" => {
                    opts.show_tabs = true;
                    continue;
                }
                "--show-nonprinting" => {
                    opts.show_nonprinting = true;
                    continue;
                }
                _ => {}
            }

            for c in arg.chars().skip(1) {
                match c {
                    'A' => {
                        opts.show_nonprinting = true;
                        opts.show_ends = true;
                        opts.show_tabs = true;
                    }
                    'b' => {
                        opts.number_nonblank = true;
                    }
                    'e' => {
                        opts.show_nonprinting = true;
                        opts.show_ends = true;
                    }
                    'E' => {
                        opts.show_ends = true;
                    }
                    'n' => {
                        opts.number_all = true;
                    }
                    's' => {
                        opts.squeeze_blank = true;
                    }
                    't' => {
                        opts.show_nonprinting = true;
                        opts.show_tabs = true;
                    }
                    'T' => {
                        opts.show_tabs = true;
                    }
                    'u' => {} // ignored in GNU
                    'v' => {
                        opts.show_nonprinting = true;
                    }
                    _ => return Err(format!("invalid option -- '{}'", c)),
                }
            }
        } else {
            opts.files.push(arg);
        }
    }
    Ok(opts)
}

fn main() -> ExitCode {
    let opts = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("cat: {}", e);
            eprintln!("Try 'cat --help' for more information.");
            return ExitCode::from(1);
        }
    };

    let stdout_lock = stdout().lock();
    let mut out = BufWriter::with_capacity(128 * 1024, stdout_lock);
    let mut line_counter: usize = 1;
    let mut prev_blank = false;
    let mut had_error = false;

    // Собираем ссылки на строки, чтобы не мувать opts.files
    let inputs: Vec<&str> = if opts.files.is_empty() {
        vec!["-"]
    } else {
        opts.files.iter().map(|s| s.as_str()).collect()
    };

    let needs_processing = opts.number_all
        || opts.number_nonblank
        || opts.squeeze_blank
        || opts.show_ends
        || opts.show_tabs
        || opts.show_nonprinting;

    for file in inputs {
        let res = if file == "-" {
            let stdin_lock = stdin().lock();
            let mut inp = BufReader::with_capacity(128 * 1024, stdin_lock);
            process(
                &mut inp,
                &mut out,
                &opts,
                &mut line_counter,
                &mut prev_blank,
            )
        } else {
            match File::open(file) {
                Ok(mut f) => {
                    if !needs_processing {
                        copy_fast(&mut f, &mut out)
                    } else {
                        let mut inp = BufReader::with_capacity(128 * 1024, f);
                        process(
                            &mut inp,
                            &mut out,
                            &opts,
                            &mut line_counter,
                            &mut prev_blank,
                        )
                    }
                }
                Err(e) => {
                    eprintln!("cat: {}: {}", file, e);
                    had_error = true;
                    Ok(())
                }
            }
        };

        if let Err(e) = res {
            eprintln!("cat: {}: {}", file, e);
            had_error = true;
        }
    }

    if out.flush().is_err() || had_error {
        ExitCode::from(1)
    } else {
        ExitCode::from(0)
    }
}

fn copy_fast<R: Read, W: Write>(reader: &mut R, writer: &mut W) -> io::Result<()> {
    let mut buf = [0u8; 128 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        writer.write_all(&buf[..n])?;
    }
    Ok(())
}

fn process<R: Read + BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    opts: &Options,
    line_counter: &mut usize,
    prev_blank: &mut bool,
) -> io::Result<()> {
    let mut buf = Vec::with_capacity(8192);
    loop {
        buf.clear();
        let n = reader.read_until(b'\n', &mut buf)?;
        if n == 0 {
            break;
        }

        let is_blank = n == 1 && buf[0] == b'\n';

        if opts.squeeze_blank && is_blank && *prev_blank {
            continue;
        }
        *prev_blank = is_blank;

        if opts.number_nonblank {
            if !is_blank {
                write!(writer, "{:>6}\t", line_counter)?;
                *line_counter += 1;
            }
        } else if opts.number_all {
            write!(writer, "{:>6}\t", line_counter)?;
            *line_counter += 1;
        }

        if opts.show_nonprinting || opts.show_tabs {
            for &b in &buf {
                if b == b'\n' {
                    break;
                }
                if opts.show_tabs && b == b'\t' {
                    writer.write_all(b"^I")?;
                } else if opts.show_nonprinting {
                    if b < 32 {
                        writer.write_all(&[b'^', b + 64])?;
                    } else if b == 127 {
                        writer.write_all(b"^?")?;
                    } else if b > 127 {
                        if b < 128 + 32 {
                            writer.write_all(&[b'M', b'-', b'^', b - 128 + 64])?;
                        } else if b == 128 + 127 {
                            writer.write_all(b"M-^?")?;
                        } else {
                            writer.write_all(&[b'M', b'-', b - 128])?;
                        }
                    } else {
                        writer.write_all(&[b])?;
                    }
                } else {
                    writer.write_all(&[b])?;
                }
            }
            if opts.show_ends {
                writer.write_all(b"$")?;
            }
            writer.write_all(b"\n")?;
        } else {
            if opts.show_ends {
                writer.write_all(&buf[..n - 1])?;
                writer.write_all(b"$\n")?;
            } else {
                writer.write_all(&buf)?;
            }
        }
    }
    Ok(())
}
