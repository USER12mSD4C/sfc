use std::borrow::Cow;
use std::collections::HashMap;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

#[cfg(unix)]
use std::os::unix::process::{CommandExt, ExitStatusExt};

use rustyline::completion::FilenameCompleter;
use rustyline::error::ReadlineError;
use rustyline::highlight::{Highlighter, MatchingBracketHighlighter};
use rustyline::{CompletionType, Config, Editor, Helper};

// Реестр запущенных фоновых процессов, которые ДОЛЖНЫ получить SIGHUP при закрытии шелла.
static mut ACTIVE_PIDS: [libc::pid_t; 1024] = [0; 1024];
static mut ACTIVE_PIDS_LEN: usize = 0;

// Сигнальный обработчик SIGHUP для шелла
extern "C" fn handle_sighup(_sig: libc::c_int) {
    unsafe {
        for i in 0..ACTIVE_PIDS_LEN {
            let pid = ACTIVE_PIDS[i];
            if pid > 0 {
                libc::kill(pid, libc::SIGHUP);
            }
        }
        libc::_exit(1);
    }
}

fn add_active_pid(pid: u32) {
    unsafe {
        if ACTIVE_PIDS_LEN < 1024 {
            ACTIVE_PIDS[ACTIVE_PIDS_LEN] = pid as libc::pid_t;
            ACTIVE_PIDS_LEN += 1;
        }
    }
}

fn remove_active_pid(pid: u32) {
    unsafe {
        let target = pid as libc::pid_t;
        if let Some(pos) = (0..ACTIVE_PIDS_LEN).find(|&i| ACTIVE_PIDS[i] == target) {
            if ACTIVE_PIDS_LEN > 0 {
                ACTIVE_PIDS[pos] = ACTIVE_PIDS[ACTIVE_PIDS_LEN - 1];
                ACTIVE_PIDS[ACTIVE_PIDS_LEN - 1] = 0;
                ACTIVE_PIDS_LEN -= 1;
            }
        }
    }
}

fn cleanup_jobs_on_exit() {
    unsafe {
        for i in 0..ACTIVE_PIDS_LEN {
            let pid = ACTIVE_PIDS[i];
            if pid > 0 {
                libc::kill(pid, libc::SIGHUP);
            }
        }
    }
}

struct Job {
    id: usize,
    child: Child,
    cmd: String,
}

fn expand_env_vars(input: &str, last_exit_code: i32) -> String {
    let mut result = String::new();
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' {
            let mut var_name = String::new();
            while let Some(&next_c) = chars.peek() {
                if next_c.is_alphanumeric() || next_c == '_' {
                    var_name.push(chars.next().unwrap());
                } else {
                    break;
                }
            }
            if !var_name.is_empty() {
                if let Ok(val) = env::var(&var_name) {
                    result.push_str(&val);
                }
            } else if chars.peek() == Some(&'?') {
                chars.next();
                result.push_str(&last_exit_code.to_string());
            } else {
                result.push('$');
            }
        } else {
            result.push(c);
        }
    }
    result
}

fn is_valid_command(cmd: &str, aliases: &HashMap<String, String>, commands: &[String]) -> bool {
    if cmd == "cd"
        || cmd == "exit"
        || cmd == "clear"
        || cmd == "jobs"
        || cmd == "disown"
        || cmd == "nopdisown"
        || aliases.contains_key(cmd)
    {
        return true;
    }
    if cmd.starts_with('.') || cmd.starts_with('/') || cmd.starts_with('~') {
        let expanded = expand_tilde(cmd);
        let path = Path::new(&expanded);
        if path.exists() {
            return true;
        }
        if let Some(parent) = path.parent() {
            let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if let Ok(entries) = fs::read_dir(parent) {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy();
                    if name_str.starts_with(file_name) {
                        return true;
                    }
                }
            }
        }
        return false;
    }

    commands
        .binary_search_by(|probe| probe.as_str().cmp(cmd))
        .is_ok()
}

fn get_all_commands(aliases: &HashMap<String, String>) -> Vec<String> {
    let mut cmds = vec![
        "cd".to_string(),
        "exit".to_string(),
        "clear".to_string(),
        "jobs".to_string(),
        "disown".to_string(),
        "nopdisown".to_string(),
    ];

    for alias in aliases.keys() {
        cmds.push(alias.clone());
    }

    if let Ok(path) = env::var("PATH") {
        for dir in env::split_paths(&path) {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    if let Ok(name) = entry.file_name().into_string() {
                        cmds.push(name);
                    }
                }
            }
        }
    }
    cmds.sort();
    cmds.dedup();
    cmds
}

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

// Вспомогательная функция для генерации подсказок по путям файлов/директорий
fn get_file_hint(current_word: &str) -> Option<CommandHint> {
    let expanded = expand_tilde(current_word);
    let path = Path::new(&expanded);

    let (dir_path, prefix) = if current_word.ends_with('/') {
        (path, "")
    } else if let Some(parent) = path.parent() {
        let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        (parent, file_name)
    } else {
        (Path::new("."), current_word)
    };

    if let Ok(entries) = fs::read_dir(dir_path) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with(prefix) && name_str != prefix {
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
            // Если ввод команды начинается с пути (./, /, ~), переключаемся на автодополнение файлов
            if current_word.starts_with('.')
                || current_word.starts_with('/')
                || current_word.starts_with('~')
            {
                return self.completer.complete(line, pos, ctx);
            }

            let mut candidates = Vec::new();
            for cmd in &self.commands {
                if cmd.starts_with(current_word) {
                    candidates.push(rustyline::completion::Pair {
                        display: cmd.clone(),
                        replacement: cmd.clone(),
                    });
                }
            }
            return Ok((start_of_word, candidates));
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
            // Если команда начинается как путь, возвращаем подсказку по путям
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
    fn highlight<'l>(&self, line: &'l str, pos: usize) -> Cow<'l, str> {
        let mut parts = line.split_whitespace();
        if let Some(first_word) = parts.next() {
            let is_valid = is_valid_command(first_word, &self.aliases, &self.commands);
            let color_code = if is_valid {
                "\x1b[38;2;166;227;161m"
            } else {
                "\x1b[38;2;243;139;168m"
            };

            let rest = &line[first_word.len()..];
            let colored = format!("{}{}{}\x1b[0m{}", color_code, "\x1b[1m", first_word, rest);
            Cow::Owned(colored)
        } else {
            self.highlighter.highlight(line, pos)
        }
    }

    fn highlight_char(&self, _line: &str, _pos: usize, _forced: bool) -> bool {
        true
    }
}

impl rustyline::validate::Validator for SFHelper {}

impl Helper for SFHelper {}

enum Redirect {
    Stdout(String),
    StdoutAppend(String),
    Stdin(String),
}

struct SimpleCommand {
    args: Vec<String>,
    redirects: Vec<Redirect>,
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Word(String),
    Pipe,
    And,
    Or,
    Semi,
    Bg,
    RedirectOut,
    RedirectAppend,
    RedirectIn,
}

fn tokenize(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut quote_char = ' ';

    while let Some(c) = chars.next() {
        if in_quotes {
            if c == quote_char {
                in_quotes = false;
            } else {
                current.push(c);
            }
        } else if c == '"' || c == '\'' {
            in_quotes = true;
            quote_char = c;
        } else if c == '#' && current.is_empty() {
            while let Some(&next_c) = chars.peek() {
                if next_c == '\n' {
                    break;
                }
                chars.next();
            }
        } else if c == '\n' {
            if !current.is_empty() {
                tokens.push(Token::Word(current.clone()));
                current.clear();
            }
            tokens.push(Token::Semi);
        } else if c.is_whitespace() {
            if !current.is_empty() {
                tokens.push(Token::Word(current.clone()));
                current.clear();
            }
        } else if c == '&' {
            if !current.is_empty() {
                tokens.push(Token::Word(current.clone()));
                current.clear();
            }
            if chars.peek() == Some(&'&') {
                chars.next();
                tokens.push(Token::And);
            } else {
                tokens.push(Token::Bg);
            }
        } else if c == '|' {
            if !current.is_empty() {
                tokens.push(Token::Word(current.clone()));
                current.clear();
            }
            if chars.peek() == Some(&'|') {
                chars.next();
                tokens.push(Token::Or);
            } else {
                tokens.push(Token::Pipe);
            }
        } else if c == ';' {
            if !current.is_empty() {
                tokens.push(Token::Word(current.clone()));
                current.clear();
            }
            tokens.push(Token::Semi);
        } else if c == '>' {
            if !current.is_empty() {
                tokens.push(Token::Word(current.clone()));
                current.clear();
            }
            if chars.peek() == Some(&'>') {
                chars.next();
                tokens.push(Token::RedirectAppend);
            } else {
                tokens.push(Token::RedirectOut);
            }
        } else if c == '<' {
            if !current.is_empty() {
                tokens.push(Token::Word(current.clone()));
                current.clear();
            }
            tokens.push(Token::RedirectIn);
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        tokens.push(Token::Word(current));
    }
    tokens
}

enum Op {
    Always,
    And,
    Or,
}

struct PipelineGroup {
    pipeline: Vec<SimpleCommand>,
    op: Op,
    background: bool,
}

fn parse_pipeline_groups(tokens: &[Token]) -> Vec<PipelineGroup> {
    let mut groups = Vec::new();
    let mut current_pipeline = Vec::new();
    let mut current_args = Vec::new();
    let mut current_redirects = Vec::new();
    let mut current_op = Op::Always;

    let mut i = 0;
    while i < tokens.len() {
        match &tokens[i] {
            Token::Word(s) => {
                let expanded = expand_tilde(s);
                current_args.push(expanded);
                i += 1;
            }
            Token::RedirectIn => {
                if i + 1 < tokens.len() {
                    if let Token::Word(path) = &tokens[i + 1] {
                        let expanded = expand_tilde(path);
                        current_redirects.push(Redirect::Stdin(expanded));
                        i += 2;
                        continue;
                    }
                }
                i += 1;
            }
            Token::RedirectOut => {
                if i + 1 < tokens.len() {
                    if let Token::Word(path) = &tokens[i + 1] {
                        let expanded = expand_tilde(path);
                        current_redirects.push(Redirect::Stdout(expanded));
                        i += 2;
                        continue;
                    }
                }
                i += 1;
            }
            Token::RedirectAppend => {
                if i + 1 < tokens.len() {
                    if let Token::Word(path) = &tokens[i + 1] {
                        let expanded = expand_tilde(path);
                        current_redirects.push(Redirect::StdoutAppend(expanded));
                        i += 2;
                        continue;
                    }
                }
                i += 1;
            }
            Token::Pipe => {
                if !current_args.is_empty() || !current_redirects.is_empty() {
                    current_pipeline.push(SimpleCommand {
                        args: current_args,
                        redirects: current_redirects,
                    });
                    current_args = Vec::new();
                    current_redirects = Vec::new();
                }
                i += 1;
            }
            Token::And | Token::Or | Token::Semi | Token::Bg => {
                if !current_args.is_empty() || !current_redirects.is_empty() {
                    current_pipeline.push(SimpleCommand {
                        args: current_args,
                        redirects: current_redirects,
                    });
                    current_args = Vec::new();
                    current_redirects = Vec::new();
                }

                let op_type = &tokens[i];
                let background = matches!(op_type, Token::Bg);

                if !current_pipeline.is_empty() {
                    groups.push(PipelineGroup {
                        pipeline: current_pipeline,
                        op: current_op,
                        background,
                    });
                    current_pipeline = Vec::new();
                }

                current_op = match op_type {
                    Token::And => Op::And,
                    Token::Or => Op::Or,
                    _ => Op::Always,
                };

                i += 1;
            }
        }
    }

    if !current_args.is_empty() || !current_redirects.is_empty() {
        current_pipeline.push(SimpleCommand {
            args: current_args,
            redirects: current_redirects,
        });
    }
    if !current_pipeline.is_empty() {
        groups.push(PipelineGroup {
            pipeline: current_pipeline,
            op: current_op,
            background: false,
        });
    }

    groups
}

fn reap_background_jobs(jobs: &mut Vec<Job>) {
    let mut i = 0;
    while i < jobs.len() {
        match jobs[i].child.try_wait() {
            Ok(Some(status)) => {
                println!(
                    "[{}] Done (exit: {})    {}",
                    jobs[i].id, status, jobs[i].cmd
                );
                let removed = jobs.remove(i);
                remove_active_pid(removed.child.id());
            }
            Ok(None) => {
                i += 1;
            }
            Err(e) => {
                eprintln!("Error polling job {}: {}", jobs[i].id, e);
                let removed = jobs.remove(i);
                remove_active_pid(removed.child.id());
            }
        }
    }
}

fn execute_pipeline(
    pipeline: Vec<SimpleCommand>,
    background: bool,
    jobs: &mut Vec<Job>,
) -> Result<i32, String> {
    if pipeline.is_empty() {
        return Ok(0);
    }

    let mut prev_stdout = None;
    let mut children: Vec<Child> = Vec::new();
    let len = pipeline.len();

    let cmd_representation = pipeline
        .iter()
        .map(|c| c.args.join(" "))
        .collect::<Vec<_>>()
        .join(" | ");

    for (idx, cmd) in pipeline.into_iter().enumerate() {
        if cmd.args.is_empty() {
            continue;
        }

        let first_arg = &cmd.args[0];

        if len == 1 {
            if first_arg == "cd" {
                let dest = cmd.args.get(1).map(|s| s.as_str()).unwrap_or("~");
                let dest_path = if dest == "~" {
                    env::var("HOME").unwrap_or_default()
                } else if dest == "-" {
                    match env::var("OLDPWD") {
                        Ok(val) => {
                            println!("{}", val);
                            val
                        }
                        Err(_) => {
                            eprintln!("cd: OLDPWD not set");
                            return Ok(1);
                        }
                    }
                } else {
                    dest.to_string()
                };

                let old_dir = env::current_dir().ok();

                if let Err(e) = env::set_current_dir(&dest_path) {
                    eprintln!("cd: {}", e);
                    return Ok(1);
                }

                if let Some(old) = old_dir {
                    env::set_var("OLDPWD", old);
                }
                if let Ok(new_dir) = env::current_dir() {
                    env::set_var("PWD", new_dir);
                }
                return Ok(0);
            } else if first_arg == "exit" {
                cleanup_jobs_on_exit();
                std::process::exit(0);
            } else if first_arg == "clear" {
                print!("\x1b[H\x1b[2J\x1b[3J");
                let _ = std::io::stdout().flush();
                return Ok(0);
            } else if first_arg == "jobs" {
                for job in jobs {
                    println!("[{}] Running          {}", job.id, job.cmd);
                }
                return Ok(0);
            } else if first_arg == "disown" {
                if cmd.args.len() == 1 {
                    if !jobs.is_empty() {
                        let removed = jobs.pop().unwrap();
                        remove_active_pid(removed.child.id());
                        println!("Disowned last job: [{}] {}", removed.id, removed.cmd);
                    } else {
                        eprintln!("disown: active jobs table is empty");
                    }
                } else {
                    let target = &cmd.args[1];
                    if target == "-a" {
                        for job in jobs.drain(..) {
                            remove_active_pid(job.child.id());
                        }
                        println!("Disowned all active background jobs.");
                    } else {
                        let id_to_remove = if target.starts_with('%') {
                            target[1..].parse::<usize>().ok()
                        } else {
                            target.parse::<usize>().ok()
                        };

                        if let Some(id) = id_to_remove {
                            if let Some(pos) = jobs.iter().position(|j| j.id == id) {
                                let removed = jobs.remove(pos);
                                remove_active_pid(removed.child.id());
                                println!("Disowned job: [{}] {}", removed.id, removed.cmd);
                            } else {
                                eprintln!("disown: {}: no such job", target);
                                return Ok(1);
                            }
                        } else if let Ok(pid) = target.parse::<u32>() {
                            if let Some(pos) = jobs.iter().position(|j| j.child.id() == pid) {
                                let removed = jobs.remove(pos);
                                remove_active_pid(removed.child.id());
                                println!("Disowned job by PID: [{}] {}", removed.id, removed.cmd);
                            } else {
                                eprintln!("disown: {}: no such job", target);
                                return Ok(1);
                            }
                        } else {
                            eprintln!("disown: {}: invalid job specification", target);
                            return Ok(1);
                        }
                    }
                }
                return Ok(0);
            } else if first_arg == "nopdisown" {
                if cmd.args.len() < 2 {
                    eprintln!("nopdisown: expected a command to run");
                    return Ok(1);
                }
                let sub_cmd = &cmd.args[1];
                let sub_args = &cmd.args[2..];

                let mut command = Command::new(sub_cmd);
                command.args(sub_args);
                command.stdin(Stdio::null());
                command.stdout(Stdio::null());
                command.stderr(Stdio::null());

                #[cfg(unix)]
                command.process_group(0);

                match command.spawn() {
                    Ok(_child) => {}
                    Err(e) => {
                        eprintln!("nopdisown: {}: {}", sub_cmd, e);
                        return Ok(1);
                    }
                }
                return Ok(0);
            }
        }

        let mut command = Command::new(&cmd.args[0]);
        let mut args = cmd.args[1..].to_vec();

        if cmd.args[0] == "nix-shell" {
            let has_command = cmd
                .args
                .iter()
                .any(|arg| arg == "--command" || arg == "--run");
            if !has_command {
                if let Ok(self_exe) = env::current_exe() {
                    if let Some(self_str) = self_exe.to_str() {
                        args.push("--command".to_string());
                        args.push(self_str.to_string());
                    }
                }
            }
        } else if cmd.args[0] == "nix" && cmd.args.get(1).map(|s| s.as_str()) == Some("develop") {
            let has_command = cmd.args.iter().any(|arg| arg == "--command" || arg == "-c");
            if !has_command {
                if let Ok(self_exe) = env::current_exe() {
                    if let Some(self_str) = self_exe.to_str() {
                        args.push("--command".to_string());
                        args.push(self_str.to_string());
                    }
                }
            }
        }

        command.args(&args);

        #[cfg(unix)]
        if background {
            command.process_group(0);
        }

        let mut stdin_set = false;
        for redir in &cmd.redirects {
            if let Redirect::Stdin(path) = redir {
                if let Ok(file) = File::open(path) {
                    command.stdin(Stdio::from(file));
                    stdin_set = true;
                } else {
                    eprintln!("SF_Shell: {}: No such file or directory", path);
                    return Ok(1);
                }
            }
        }

        if !stdin_set {
            if let Some(prev) = prev_stdout {
                command.stdin(Stdio::from(prev));
            } else {
                command.stdin(Stdio::inherit());
            }
        }

        let mut stdout_set = false;
        for redir in &cmd.redirects {
            match redir {
                Redirect::Stdout(path) => {
                    if let Ok(file) = File::create(path) {
                        command.stdout(Stdio::from(file));
                        stdout_set = true;
                    }
                }
                Redirect::StdoutAppend(path) => {
                    if let Ok(file) = OpenOptions::new().create(true).append(true).open(path) {
                        command.stdout(Stdio::from(file));
                        stdout_set = true;
                    }
                }
                _ => {}
            }
        }

        let mut next_stdout = None;
        if !stdout_set {
            if idx < len - 1 {
                command.stdout(Stdio::piped());
            } else {
                command.stdout(Stdio::inherit());
            }
        }

        command.stderr(Stdio::inherit());

        match command.spawn() {
            Ok(mut child) => {
                if idx < len - 1 && !stdout_set {
                    next_stdout = child.stdout.take();
                }
                children.push(child);
            }
            Err(e) => {
                eprintln!("SF_Shell: {}: {}", cmd.args[0], e);
                return Ok(1);
            }
        }

        prev_stdout = next_stdout;
    }

    if background {
        if let Some(last_child) = children.pop() {
            let next_id = jobs.iter().map(|j| j.id).max().unwrap_or(0) + 1;
            println!("[{}] {}", next_id, last_child.id());
            add_active_pid(last_child.id());
            jobs.push(Job {
                id: next_id,
                child: last_child,
                cmd: cmd_representation,
            });
        }
        return Ok(0);
    }

    unsafe {
        libc::signal(libc::SIGINT, libc::SIG_IGN);
    }

    let mut last_status = 0;
    for (idx, mut child) in children.into_iter().enumerate() {
        match child.wait() {
            Ok(status) => {
                if idx == len - 1 {
                    // Корректная обработка кода возврата:
                    // Если процесс завершился нормально, берем его код.
                    // Если процесс аварийно завершился по сигналу, вычисляем как 128 + номер_сигнала.
                    last_status = if let Some(code) = status.code() {
                        code
                    } else {
                        #[cfg(unix)]
                        {
                            status.signal().map(|sig| 128 + sig).unwrap_or(1)
                        }
                        #[cfg(not(unix))]
                        {
                            1
                        }
                    };
                }
            }
            Err(e) => {
                eprintln!("Error waiting for process: {}", e);
            }
        }
    }

    unsafe {
        libc::signal(libc::SIGINT, libc::SIG_DFL);
    }

    Ok(last_status)
}

fn execute_groups(groups: Vec<PipelineGroup>, jobs: &mut Vec<Job>) -> i32 {
    let mut last_exit_code = 0;
    for group in groups {
        let should_run = match group.op {
            Op::Always => true,
            Op::And => last_exit_code == 0,
            Op::Or => last_exit_code != 0,
        };

        if should_run {
            match execute_pipeline(group.pipeline, group.background, jobs) {
                Ok(code) => {
                    last_exit_code = code;
                }
                Err(_) => {
                    last_exit_code = 1;
                }
            }
        }
    }
    last_exit_code
}

fn get_prompt(last_exit_code: i32) -> String {
    let username = env::var("USER").unwrap_or_else(|_| "user".to_string());

    let in_nix = env::var("IN_NIX_SHELL").is_ok();
    let host_display = if in_nix {
        "\x1b[1;32mnixshell\x1b[0m".to_string()
    } else {
        fs::read_to_string("/proc/sys/kernel/hostname")
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "nixos".to_string())
    };

    let current_dir = env::current_dir()
        .ok()
        .and_then(|p| {
            let home = env::var("HOME").unwrap_or_default();
            let path_str = p.to_str()?;
            if path_str == home {
                Some("~".to_string())
            } else if path_str.starts_with(&home) {
                Some(format!("~{}", &path_str[home.len()..]))
            } else {
                Some(path_str.to_string())
            }
        })
        .unwrap_or_else(|| "~".to_string());

    let status_color = if last_exit_code == 0 {
        "\x1b[38;2;166;227;161m"
    } else {
        "\x1b[38;2;243;139;168m"
    };

    format!(
        "\x1b[38;2;203;166;247m{{\x1b[0m{}@{}; {}\x1b[38;2;203;166;247m}}\x1b[0m{}$\x1b[0m ",
        username, host_display, current_dir, status_color
    )
}

fn load_sfsrc(jobs: &mut Vec<Job>) -> HashMap<String, String> {
    let mut aliases = HashMap::new();
    let home = env::var("HOME").unwrap_or_default();
    let path = Path::new(&home).join(".sfsrc");

    if let Ok(content) = fs::read_to_string(path) {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if trimmed.starts_with("alias ") {
                let parts: Vec<&str> = trimmed["alias ".len()..].splitn(2, '=').collect();
                if parts.len() == 2 {
                    let key = parts[0].trim().to_string();
                    let val = parts[1]
                        .trim()
                        .trim_matches('"')
                        .trim_matches('\'')
                        .to_string();
                    let expanded_val = expand_env_vars(&val, 0);
                    aliases.insert(key, expanded_val);
                }
            } else if trimmed.starts_with("export ") {
                let parts: Vec<&str> = trimmed["export ".len()..].splitn(2, '=').collect();
                if parts.len() == 2 {
                    let key = parts[0].trim().to_string();
                    let val = parts[1]
                        .trim()
                        .trim_matches('"')
                        .trim_matches('\'')
                        .to_string();
                    let expanded_val = expand_env_vars(&val, 0);
                    env::set_var(key, expanded_val);
                }
            } else {
                let expanded_line = expand_alias(trimmed, &aliases);
                let expanded_with_env = expand_env_vars(&expanded_line, 0);
                let tokens = tokenize(&expanded_with_env);
                let groups = parse_pipeline_groups(&tokens);
                execute_groups(groups, jobs);
            }
        }
    }
    aliases
}

fn expand_alias(line: &str, aliases: &HashMap<String, String>) -> String {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return trimmed.to_string();
    }
    let first_word = trimmed.split_whitespace().next().unwrap_or("");
    if let Some(expanded) = aliases.get(first_word) {
        let remainder = &trimmed[first_word.len()..];
        format!("{}{}", expanded, remainder)
    } else {
        trimmed.to_string()
    }
}

fn expand_tilde(input: &str) -> String {
    if input == "~" {
        env::var("HOME").unwrap_or_default()
    } else if input.starts_with("~/") {
        let home = env::var("HOME").unwrap_or_default();
        format!("{}{}", home, &input[1..])
    } else {
        input.to_string()
    }
}

fn main() -> Result<(), ReadlineError> {
    unsafe {
        libc::signal(libc::SIGINT, libc::SIG_IGN);
        libc::signal(
            libc::SIGHUP,
            handle_sighup as *const () as libc::sighandler_t,
        );
    }

    let mut jobs: Vec<Job> = Vec::new();
    let mut last_exit_code = 0;

    let args: Vec<String> = env::args().collect();
    if args.len() > 2 && args[1] == "-c" {
        let command_line = &args[2];
        let tokens = tokenize(command_line);
        let groups = parse_pipeline_groups(&tokens);
        execute_groups(groups, &mut jobs);
        return Ok(());
    }

    let aliases = load_sfsrc(&mut jobs);
    let commands = get_all_commands(&aliases);

    let config = Config::builder()
        .completion_type(CompletionType::List)
        .build();

    let mut rl = Editor::with_config(config)?;
    let h = SFHelper {
        completer: FilenameCompleter::new(),
        highlighter: MatchingBracketHighlighter::new(),
        commands,
        aliases: aliases.clone(),
    };
    rl.set_helper(Some(h));

    let history_path = env::var("HOME")
        .map(|h| PathBuf::from(h).join(".sf_history"))
        .ok();

    if let Some(ref path) = history_path {
        let _ = rl.load_history(path);
    }

    loop {
        reap_background_jobs(&mut jobs);

        let prompt = get_prompt(last_exit_code);
        let readline = rl.readline(&prompt);
        match readline {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                let _ = rl.add_history_entry(trimmed);
                if let Some(ref path) = history_path {
                    let _ = rl.save_history(path);
                }

                let expanded_alias = expand_alias(trimmed, &aliases);
                let expanded_line = expand_env_vars(&expanded_alias, last_exit_code);

                let tokens = tokenize(&expanded_line);
                let groups = parse_pipeline_groups(&tokens);

                last_exit_code = execute_groups(groups, &mut jobs);
            }
            Err(ReadlineError::Interrupted) => {
                println!("^C");
            }
            Err(ReadlineError::Eof) => {
                println!("exit");
                cleanup_jobs_on_exit();
                break;
            }
            Err(err) => {
                eprintln!("Error: {:?}", err);
                cleanup_jobs_on_exit();
                break;
            }
        }
    }
    Ok(())
}
