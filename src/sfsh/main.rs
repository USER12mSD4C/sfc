use crate::sfsh::ast::Command as AstCommand;
use crate::sfsh::exec::{execute_script, shell_exit};
use crate::sfsh::job::JobTable;
use crate::sfsh::signal::setup_signals;
use crate::sfsh::vars::ShellVars;
use rustyline::completion::FilenameCompleter;
use rustyline::error::ReadlineError;
use rustyline::highlight::{Highlighter, MatchingBracketHighlighter};
use rustyline::{CompletionType, Config, Editor, Helper};
use std::borrow::Cow;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

static SIGHUP_RECEIVED: AtomicBool = AtomicBool::new(false);

extern "C" fn handle_sighup(_sig: libc::c_int) {
    SIGHUP_RECEIVED.store(true, Ordering::SeqCst);
}

struct SFHelper {
    completer: FilenameCompleter,
    highlighter: MatchingBracketHighlighter,
    commands: Vec<String>,
    aliases: HashMap<String, String>,
    heredoc_delimiter: std::cell::RefCell<Option<String>>,
}

impl rustyline::completion::Completer for SFHelper {
    type Candidate = rustyline::completion::Pair;
    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        let trimmed = &line[..pos];
        let (start, word) = match trimmed.rfind(|c: char| c.is_whitespace()) {
            Some(i) => (i + 1, &trimmed[i + 1..]),
            None => (0, trimmed),
        };
        let before = trimmed[..start].trim();
        let is_cmd = start == 0
            || before
                .split_whitespace()
                .last()
                .map(|w| matches!(w, "sudo" | "doas" | "nohup" | "stdbuf"))
                .unwrap_or(false);

        if word.starts_with('$') {
            let prefix = &word[1..];
            let mut cands = Vec::new();
            for (k, _) in env::vars() {
                if k.starts_with(prefix) {
                    cands.push(rustyline::completion::Pair {
                        display: format!("${}", k),
                        replacement: format!("${}", k),
                    });
                }
            }
            return Ok((start, cands));
        }

        if is_cmd && !word.starts_with(|c: char| c == '.' || c == '/' || c == '~') {
            let mut cands = Vec::new();
            for cmd in &self.commands {
                if cmd.starts_with(word) {
                    cands.push(rustyline::completion::Pair {
                        display: cmd.clone(),
                        replacement: cmd.clone(),
                    });
                }
            }
            return Ok((start, cands));
        }
        self.completer.complete(line, pos, ctx)
    }
}

impl rustyline::hint::Hinter for SFHelper {
    type Hint = CommandHint;

    fn hint(&self, line: &str, pos: usize, _ctx: &rustyline::Context<'_>) -> Option<Self::Hint> {
        let trimmed = &line[..pos];
        if trimmed.trim().is_empty() || pos < line.len() {
            return None;
        }

        let (start_of_word, current_word) = match trimmed.rfind(' ') {
            Some(idx) => (idx + 1, &trimmed[idx + 1..]),
            None => (0, trimmed),
        };

        let before_word = trimmed[..start_of_word].trim();
        let mut is_command_position = start_of_word == 0;

        if !is_command_position && !before_word.is_empty() {
            let words: Vec<&str> = before_word.split_whitespace().collect();
            if let Some(last_non_flag) = words.iter().rev().find(|&&w| !w.starts_with('-')) {
                if *last_non_flag == "sudo"
                    || *last_non_flag == "doas"
                    || *last_non_flag == "stdbuf"
                    || *last_non_flag == "nohup"
                {
                    is_command_position = true;
                }
            }
        }

        if is_command_position {
            if current_word.starts_with('.')
                || current_word.starts_with('/')
                || current_word.starts_with('~')
            {
                return get_file_hint(current_word);
            }

            for cmd in &self.commands {
                if cmd.starts_with(current_word) && cmd != current_word {
                    let hint_str = cmd[current_word.len()..].to_string();
                    return Some(CommandHint {
                        display: format!("\x1b[38;2;90;90;90m{}\x1b[0m", hint_str),
                        completion: hint_str,
                    });
                }
            }
            return None;
        }

        get_file_hint(current_word)
    }
}

impl Highlighter for SFHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        if self.heredoc_delimiter.borrow().is_some() {
            return Cow::Borrowed(line);
        }

        let trimmed = line.trim_start();
        let leading = &line[..line.len() - trimmed.len()];

        let mut parts = trimmed.split_whitespace();
        if let Some(first) = parts.next() {
            let valid = is_valid_cmd(first, &self.aliases, &self.commands);
            let color = if valid {
                "\x1b[38;2;166;227;161m\x1b[1m"
            } else {
                "\x1b[38;2;243;139;168m\x1b[1m"
            };
            let rest = &trimmed[first.len()..];
            Cow::Owned(format!("{}{}{}\x1b[0m{}", leading, color, first, rest))
        } else {
            self.highlighter.highlight(line, _pos)
        }
    }
    fn highlight_char(&self, _line: &str, _pos: usize, _forced: bool) -> bool {
        true
    }
}

impl rustyline::validate::Validator for SFHelper {}
impl Helper for SFHelper {}

#[derive(Hash, Debug, PartialEq, Eq, Clone)]
struct CommandHint {
    display: String,
    completion: String,
}

impl rustyline::hint::Hint for CommandHint {
    fn display(&self) -> &str {
        &self.display
    }
    fn completion(&self) -> Option<&str> {
        Some(&self.completion)
    }
}

use std::sync::OnceLock;

fn get_hostname() -> &'static str {
    static HOSTNAME: OnceLock<String> = OnceLock::new();
    HOSTNAME
        .get_or_init(|| {
            fs::read_to_string("/proc/sys/kernel/hostname")
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| "unknown".to_string())
        })
        .as_str()
}

fn is_valid_cmd(cmd: &str, aliases: &HashMap<String, String>, commands: &[String]) -> bool {
    if matches!(
        cmd,
        "cd" | "exit"
            | "clear"
            | "jobs"
            | "disown"
            | "unset"
            | "export"
            | "alias"
            | "unalias"
            | "source"
            | "."
            | "eval"
            | "exec"
            | "set"
            | "shift"
            | "read"
            | "local"
            | "test"
            | "["
            | "hash"
            | "type"
            | "umask"
            | "trap"
            | "wait"
            | "fg"
            | "bg"
            | "return"
            | "break"
            | "continue"
            | ":"
            | "echo"
            | "true"
            | "false"
            | "printf"
            | "pwd"
            | "command"
            | "readonly"
            | "times"
            | "getopts"
            | "lem"
            | "if"
            | "then"
            | "else"
            | "elif"
            | "fi"
            | "for"
            | "while"
            | "until"
            | "do"
            | "done"
            | "case"
            | "esac"
            | "function"
            | "in"
            | "select"
            | "time"
            | "{"
            | "}"
            | "!"
    ) {
        return true;
    }
    if aliases.contains_key(cmd) {
        return true;
    }
    if cmd.starts_with('.') || cmd.starts_with('/') || cmd.starts_with('~') {
        let expanded = expand_tilde(cmd);
        let p = Path::new(&expanded);
        if p.exists() {
            return true;
        }
    }
    commands
        .binary_search_by(|probe| probe.as_str().cmp(cmd))
        .is_ok()
}

fn expand_tilde(input: &str) -> String {
    if input == "~" {
        env::var("HOME").unwrap_or_default()
    } else if input.starts_with("~/") {
        format!("{}{}", env::var("HOME").unwrap_or_default(), &input[1..])
    } else {
        input.to_string()
    }
}

fn get_all_commands(aliases: &HashMap<String, String>) -> Vec<String> {
    let cmds = vec![
        "cd", "exit", "clear", "jobs", "disown", "unset", "export", "alias", "unalias", "source",
        "eval", "exec", "set", "shift", "read", "local", "test", "[", "hash", "type", "umask",
        "trap", "wait", "fg", "bg", "return", "break", "continue", "true", "false", "printf",
        "echo", ":", ".", "pwd", "command", "readonly", "times", "getopts", "lem", "if", "then",
        "else", "elif", "fi", "for", "while", "until", "do", "done", "case", "esac", "function",
        "in", "select", "time",
    ];
    let mut res: Vec<String> = cmds.into_iter().map(|s| s.to_string()).collect();
    for a in aliases.keys() {
        res.push(a.clone());
    }
    if let Ok(path) = env::var("PATH") {
        for dir in env::split_paths(&path) {
            if let Ok(entries) = fs::read_dir(dir) {
                for e in entries.flatten() {
                    if let Ok(name) = e.file_name().into_string() {
                        res.push(name);
                    }
                }
            }
        }
    }
    res.sort();
    res.dedup();
    res
}

fn history_expand(line: &str, last_cmd: &str) -> String {
    if line.starts_with("!!") {
        return format!("{}{}", last_cmd, &line[2..]);
    }
    if line.starts_with("!$") {
        let parts: Vec<&str> = last_cmd.split_whitespace().collect();
        if let Some(last) = parts.last() {
            return format!("{}{}", last, &line[2..]);
        }
    }
    if line.starts_with("!-1") {
        return format!("{}{}", last_cmd, &line[3..]);
    }
    line.to_string()
}

fn expand_ps(ps: &str, vars: &ShellVars) -> String {
    let mut res = String::new();
    let mut chars = ps.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('u') => res.push_str(&env::var("USER").unwrap_or_default()),
                Some('h') => res.push_str(get_hostname()),
                Some('w') => res.push_str(
                    &env::current_dir()
                        .ok()
                        .and_then(|p| p.to_str().map(|s| s.to_string()))
                        .unwrap_or_default(),
                ),
                Some('W') => {
                    let cur = env::current_dir().ok();
                    let name = cur
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .and_then(|n| n.to_str())
                        .unwrap_or("~");
                    res.push_str(name);
                }
                Some('$') => res.push('$'),
                Some('!') => res.push_str(&vars.last_bg_pid.to_string()),
                Some('?') => res.push_str(&vars.last_status.to_string()),
                Some('s') => res.push_str("sfsh"),
                Some('v') => res.push_str("0.1"),
                Some(x) => {
                    res.push('\\');
                    res.push(x);
                }
                None => res.push('\\'),
            }
        } else if c == '$' && chars.peek() == Some(&'?') {
            chars.next();
            res.push_str(&vars.last_status.to_string());
        } else {
            res.push(c);
        }
    }
    res
}

fn get_prompt(vars: &ShellVars, ps2: bool) -> String {
    if ps2 {
        return vars.get("PS2").unwrap_or_else(|| ">> ".to_string());
    }
    if let Some(ps1) = vars.get("PS1") {
        if !ps1.is_empty() {
            return expand_ps(&ps1, vars);
        }
    }

    let user = env::var("USER").unwrap_or_else(|_| "user".to_string());
    let dir = env::current_dir()
        .ok()
        .and_then(|p| {
            let home = env::var("HOME").unwrap_or_default();
            let s = p.to_str()?;
            if s == home {
                Some("~".to_string())
            } else if s.starts_with(&home) {
                Some(format!("~{}", &s[home.len()..]))
            } else {
                Some(s.to_string())
            }
        })
        .unwrap_or_else(|| "~".to_string());

    let col = if vars.last_status == 0 {
        "\x1b[38;2;166;227;161m"
    } else {
        "\x1b[38;2;243;139;168m"
    };

    let lem_env = std::env::var("LEM_ENV").unwrap_or_default();
    let host = if lem_env.is_empty() {
        get_hostname().to_string()
    } else {
        lem_env
    };

    format!(
        "\x1b[38;2;203;166;247m{{\x1b[0m{}@{}; {}\x1b[38;2;203;166;247m}}\x1b[0m{}$\x1b[0m ",
        user, host, dir, col
    )
}

fn load_sfsrc(
    vars: &mut ShellVars,
    aliases: &mut HashMap<String, String>,
    funcs: &mut HashMap<String, AstCommand>,
    jobs: &mut JobTable,
    shell_pgid: i32,
) {
    let home = env::var("HOME").unwrap_or_default();
    let path = Path::new(&home).join(".sfsrc");
    if let Ok(content) = fs::read_to_string(path) {
        execute_script(&content, vars, aliases, funcs, jobs, shell_pgid);
    }
}

fn find_heredoc_in_line(line: &str) -> Option<(usize, bool, String, usize)> {
    let mut in_quote = false;
    let mut quote_char = ' ';
    let bytes = line.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let c = bytes[i] as char;

        if in_quote {
            if c == quote_char {
                in_quote = false;
            }
            i += 1;
            continue;
        }

        if c == '"' || c == '\'' {
            in_quote = true;
            quote_char = c;
            i += 1;
            continue;
        }

        if c == '\\' {
            i += 2;
            continue;
        }

        if c == '<' && i + 1 < bytes.len() && bytes[i + 1] == b'<' {
            let strip = i + 2 < bytes.len() && bytes[i + 2] == b'-';
            let start = if strip { i + 3 } else { i + 2 };

            let rest = std::str::from_utf8(&bytes[start..])
                .unwrap_or("")
                .trim_start();

            let raw_delim = rest.split_whitespace().next()?;
            let delim = raw_delim.trim_matches('"').trim_matches('\'');

            return Some((i, strip, delim.to_string(), raw_delim.len()));
        }

        i += 1;
    }

    None
}

fn handle_heredocs(
    input: &str,
    rl: &mut Editor<SFHelper, rustyline::history::DefaultHistory>,
) -> String {
    let lines: Vec<&str> = input.lines().collect();
    let mut result = String::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        if let Some((pos, strip, delim, delim_raw_len)) = find_heredoc_in_line(line) {
            i += 1;

            let mut content = String::new();
            let mut found = false;
            while i < lines.len() {
                let l = lines[i];
                let trimmed = if strip { l.trim_start() } else { l };
                if trimmed == delim {
                    found = true;
                    i += 1;
                    break;
                }
                content.push_str(l);
                content.push('\n');
                i += 1;
            }

            if !found {
                loop {
                    let prompt = format!("{}> ", delim);
                    match rl.readline(&prompt) {
                        Ok(l) => {
                            let trimmed = if strip { l.trim_start() } else { l.as_str() };
                            if trimmed == delim {
                                break;
                            }
                            content.push_str(&l);
                            content.push('\n');
                        }
                        Err(ReadlineError::Interrupted) => {
                            println!("^C");
                            content.clear();
                            break;
                        }
                        Err(ReadlineError::Eof) => break,
                        Err(_) => break,
                    }
                }
            }

            let tmp = format!(
                "/tmp/sfsh_heredoc_{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            let _ = std::fs::write(&tmp, &content);

            let mut skip = pos;
            let bytes = line.as_bytes();
            if skip + 1 < bytes.len() && bytes[skip] == b'<' && bytes[skip + 1] == b'<' {
                skip += 2;
                if skip < bytes.len() && bytes[skip] == b'-' {
                    skip += 1;
                }
                while skip < bytes.len() && (bytes[skip] == b' ' || bytes[skip] == b'\t') {
                    skip += 1;
                }
                skip += delim_raw_len;
            }
            let remaining_on_line = &line[skip..];
            result.push_str(&line[..pos]);
            result.push_str(&format!("< {}", tmp));
            result.push_str(remaining_on_line);
            result.push('\n');
        } else {
            result.push_str(line);
            result.push('\n');
            i += 1;
        }
    }

    if result.ends_with('\n') {
        result.pop();
    }
    result
}

pub fn sfsh_main() -> Result<(), ReadlineError> {
    let shell_pgid = unsafe { libc::getpgrp() };

    let _sig_r = setup_signals();

    unsafe {
        libc::signal(
            libc::SIGHUP,
            handle_sighup as *const () as libc::sighandler_t,
        );
    }

    let mut vars = ShellVars::new();
    let mut aliases: HashMap<String, String> = HashMap::new();
    let mut funcs: HashMap<String, AstCommand> = HashMap::new();
    let mut jobs = JobTable::new();

    let args: Vec<String> = env::args().collect();

    if args.len() > 2 && args[1] == "-c" {
        let code = execute_script(
            &args[2],
            &mut vars,
            &mut aliases,
            &mut funcs,
            &mut jobs,
            shell_pgid,
        );
        shell_exit(code);
    }

    if args.len() == 2 {
        let file = &args[1];
        if let Ok(content) = std::fs::read_to_string(file) {
            let code = execute_script(
                &content,
                &mut vars,
                &mut aliases,
                &mut funcs,
                &mut jobs,
                shell_pgid,
            );
            shell_exit(code);
        } else {
            eprintln!("sfsh: {}: cannot open", file);
            shell_exit(127);
        }
    }

    load_sfsrc(&mut vars, &mut aliases, &mut funcs, &mut jobs, shell_pgid);

    let config = Config::builder()
        .completion_type(CompletionType::List)
        .build();
    let mut rl = Editor::with_config(config)?;
    let h = SFHelper {
        completer: FilenameCompleter::new(),
        highlighter: MatchingBracketHighlighter::new(),
        commands: get_all_commands(&aliases),
        aliases: aliases.clone(),
        heredoc_delimiter: std::cell::RefCell::new(None),
    };
    rl.set_helper(Some(h));

    let history_path = env::var("HOME")
        .map(|h| PathBuf::from(h).join(".sf_history"))
        .ok();
    if let Some(ref p) = history_path {
        let _ = rl.load_history(p);
    }

    let mut last_cmd = String::new();

    let mut accumulated_line = String::new();
    let mut is_continuation = false;

    loop {
        if SIGHUP_RECEIVED.load(Ordering::SeqCst) {
            cleanup_jobs_on_sighup(&mut jobs);
            break;
        }

        let prompt = if is_continuation {
            "\x1b[38;2;137;180;250m>>\x1b[0m ".to_string()
        } else {
            get_prompt(&vars, false)
        };

        match rl.readline(&prompt) {
            Ok(line) => {
                let trimmed = line.trim_end();

                if ends_with_continuation(trimmed) {
                    accumulated_line.push_str(&trimmed[..trimmed.len() - 1]);
                    accumulated_line.push(' ');
                    is_continuation = true;
                    continue;
                }

                if is_continuation {
                    accumulated_line.push_str(trimmed);
                    is_continuation = false;
                } else {
                    accumulated_line = trimmed.to_string();
                }

                let full_line = accumulated_line.trim();

                if full_line.is_empty() {
                    accumulated_line.clear();
                    continue;
                }

                let _ = rl.add_history_entry(full_line);
                if let Some(ref p) = history_path {
                    let _ = rl.save_history(p);
                }

                let expanded = history_expand(full_line, &last_cmd);
                last_cmd = expanded.clone();

                if let Some(delim) = detect_heredoc_start(&expanded) {
                    if let Some(helper) = rl.helper_mut() {
                        *helper.heredoc_delimiter.borrow_mut() = Some(delim);
                    }
                }

                let processed = handle_heredocs(&expanded, &mut rl);

                if let Some(helper) = rl.helper_mut() {
                    *helper.heredoc_delimiter.borrow_mut() = None;
                }
                let code = execute_script(
                    &processed,
                    &mut vars,
                    &mut aliases,
                    &mut funcs,
                    &mut jobs,
                    shell_pgid,
                );
                vars.last_status = code;
                accumulated_line.clear();
            }
            Err(ReadlineError::Interrupted) => {
                println!("^C");
                accumulated_line.clear();
                is_continuation = false;
            }
            Err(ReadlineError::Eof) => {
                println!("exit");
                break;
            }
            Err(e) => {
                eprintln!("Error: {:?}", e);
                break;
            }
        }
    }

    for (_, job) in &jobs.jobs {
        unsafe {
            libc::kill(job.pgid.as_raw(), libc::SIGHUP);
        }
    }

    Ok(())
}

fn cleanup_jobs_on_sighup(jobs: &mut JobTable) {
    for (_, job) in &jobs.jobs {
        unsafe {
            libc::kill(job.pgid.as_raw(), libc::SIGHUP);
        }
    }
}

fn ends_with_continuation(s: &str) -> bool {
    let backslash_count = s.chars().rev().take_while(|&c| c == '\\').count();
    backslash_count % 2 == 1
}

fn get_file_hint(current_word: &str) -> Option<CommandHint> {
    let expanded = expand_tilde(current_word);
    let path = Path::new(&expanded);

    let (dir_path, prefix) = if current_word.ends_with('/') {
        (path.to_path_buf(), String::new())
    } else if let Some(parent) = path.parent() {
        let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        (parent.to_path_buf(), file_name.to_string())
    } else {
        (PathBuf::from("."), current_word.to_string())
    };

    if let Ok(entries) = std::fs::read_dir(&dir_path) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with(&prefix) && name_str != prefix {
                if name_str.starts_with('.') && !prefix.starts_with('.') {
                    continue;
                }
                let suffix = &name_str[prefix.len()..];
                let mut hint_str = suffix.to_string();
                if entry.path().is_dir() {
                    hint_str.push('/');
                }
                return Some(CommandHint {
                    display: format!("\x1b[38;2;90;90;90m{}\x1b[0m", hint_str),
                    completion: hint_str,
                });
            }
        }
    }
    None
}

fn detect_heredoc_start(line: &str) -> Option<String> {
    let mut in_quote = false;
    let mut quote_char = ' ';
    let bytes = line.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let c = bytes[i] as char;

        if in_quote {
            if c == quote_char {
                in_quote = false;
            }
            i += 1;
            continue;
        }

        if c == '"' || c == '\'' {
            in_quote = true;
            quote_char = c;
            i += 1;
            continue;
        }

        if c == '\\' {
            i += 2;
            continue;
        }

        if c == '<' && i + 1 < bytes.len() && bytes[i + 1] == b'<' {
            let strip = i + 2 < bytes.len() && bytes[i + 2] == b'-';
            let start = if strip { i + 3 } else { i + 2 };
            let rest = std::str::from_utf8(&bytes[start..])
                .unwrap_or("")
                .trim_start();
            let delim = rest.split_whitespace().next()?;
            let delim = delim.trim_matches('"').trim_matches('\'');
            return Some(delim.to_string());
        }

        i += 1;
    }

    None
}
