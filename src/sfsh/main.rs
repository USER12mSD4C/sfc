use crate::sfsh::ast::Command as AstCommand;
use crate::sfsh::exec::execute_script;
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

struct SFHelper {
    completer: FilenameCompleter,
    highlighter: MatchingBracketHighlighter,
    commands: Vec<String>,
    aliases: HashMap<String, String>,
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
        let (start, word) = match trimmed.rfind(' ') {
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
        if is_cmd && !word.starts_with(|c: char| c == '.' || c == '/' || c == '~') {
            for cmd in &self.commands {
                if cmd.starts_with(word) && cmd != word {
                    return Some(CommandHint {
                        display: format!("\x1b[38;2;90;90;90m{}\x1b[0m", &cmd[word.len()..]),
                        completion: cmd[word.len()..].to_string(),
                    });
                }
            }
        }
        None
    }
}

impl Highlighter for SFHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        let mut parts = line.split_whitespace();
        if let Some(first) = parts.next() {
            let valid = is_valid_cmd(first, &self.aliases, &self.commands);
            let color = if valid {
                "\x1b[38;2;166;227;161m\x1b[1m"
            } else {
                "\x1b[38;2;243;139;168m\x1b[1m"
            };
            Cow::Owned(format!("{}{}\x1b[0m{}", color, first, &line[first.len()..]))
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
        "echo", ":", ".",
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
                Some('h') => res.push_str(
                    &fs::read_to_string("/proc/sys/kernel/hostname")
                        .map(|s| s.trim().to_string())
                        .unwrap_or_default(),
                ),
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
        return vars.get("PS2").unwrap_or_else(|| "> ".to_string());
    }
    if let Some(ps1) = vars.get("PS1") {
        if !ps1.is_empty() {
            return expand_ps(&ps1, vars);
        }
    }
    let user = env::var("USER").unwrap_or_else(|_| "user".to_string());

    let in_nix = env::var("IN_NIX_SHELL").is_ok();
    let host = if in_nix {
        "\x1b[1;32mnixshell\x1b[0m".to_string()
    } else {
        fs::read_to_string("/proc/sys/kernel/hostname")
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "host".to_string())
    };

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

fn find_heredoc_in_line(line: &str) -> Option<(usize, bool, String)> {
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
            return Some((i, strip, delim.to_string()));
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
        if let Some((pos, strip, delim)) = find_heredoc_in_line(line) {
            result.push_str(&line[..pos]);
            result.push_str("<<");
            if strip {
                result.push('-');
            }
            result.push_str(&delim);
            result.push('\n');
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
            result.push_str(&format!("< {}", tmp));
            if i < lines.len() {
                result.push('\n');
            }
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

    let mut vars = ShellVars::new();
    let mut aliases: HashMap<String, String> = HashMap::new();
    let mut funcs: HashMap<String, AstCommand> = HashMap::new();
    let mut jobs = JobTable::new();

    let _sig_r = setup_signals();

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
        std::process::exit(code);
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
    };
    rl.set_helper(Some(h));

    let history_path = env::var("HOME")
        .map(|h| PathBuf::from(h).join(".sf_history"))
        .ok();
    if let Some(ref p) = history_path {
        let _ = rl.load_history(p);
    }

    let mut last_cmd = String::new();

    loop {
        let prompt = get_prompt(&vars, false);
        match rl.readline(&prompt) {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(line);
                if let Some(ref p) = history_path {
                    let _ = rl.save_history(p);
                }

                let expanded = history_expand(line, &last_cmd);
                last_cmd = expanded.clone();

                let processed = handle_heredocs(&expanded, &mut rl);
                let code = execute_script(
                    &processed,
                    &mut vars,
                    &mut aliases,
                    &mut funcs,
                    &mut jobs,
                    shell_pgid,
                );
                vars.last_status = code;
            }
            Err(ReadlineError::Interrupted) => {
                println!("^C");
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
