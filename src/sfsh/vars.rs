use std::collections::{HashMap, HashSet};
use std::env;

pub struct ShellVars {
    pub vars: HashMap<String, String>,
    pub exported: HashMap<String, String>,
    pub positional: Vec<String>,
    pub last_status: i32,
    pub last_bg_pid: u32,
    pub shell_pid: u32,
    pub opts: HashMap<char, bool>,
    pub readonly: HashSet<String>,
    pub local_stack: Vec<HashMap<String, String>>,
    pub argv0: String,
}

impl ShellVars {
    pub fn new() -> Self {
        let mut exported = HashMap::new();
        for (k, v) in env::vars() {
            exported.insert(k, v);
        }

        let mut opts = HashMap::new();
        opts.insert('e', false);
        opts.insert('u', false);
        opts.insert('x', false);
        opts.insert('f', false);
        opts.insert('p', false);
        opts.insert('E', false);

        Self {
            vars: HashMap::new(),
            exported,
            positional: Vec::new(),
            last_status: 0,
            last_bg_pid: 0,
            shell_pid: std::process::id(),
            opts,
            readonly: HashSet::new(),
            local_stack: Vec::new(),
            argv0: std::env::args()
                .next()
                .unwrap_or_else(|| "sfsh".to_string()),
        }
    }

    pub fn get(&self, name: &str) -> Option<String> {
        match name {
            "?" => Some(self.last_status.to_string()),
            "$" => Some(self.shell_pid.to_string()),
            "!" => Some(self.last_bg_pid.to_string()),
            "#" => Some(self.positional.len().to_string()),
            "-" => {
                let mut s = String::new();
                for (c, on) in &self.opts {
                    if *on {
                        s.push(*c);
                    }
                }
                Some(s)
            }
            "0" => Some(self.argv0.clone()),
            "@" | "*" => {
                if self.positional.is_empty() {
                    Some(String::new())
                } else {
                    Some(self.positional.join(" "))
                }
            }
            _ if name.chars().all(|c| c.is_ascii_digit()) && !name.is_empty() => {
                let n: usize = name.parse().unwrap_or(0);
                if n == 0 {
                    Some(self.argv0.clone())
                } else {
                    self.positional.get(n.saturating_sub(1)).cloned()
                }
            }
            _ if name.contains('[') && name.ends_with(']') => {
                let base = name.split('[').next().unwrap_or("");

                match base {
                    "BASH_SOURCE" => Some(
                        self.get("0")
                            .unwrap_or_else(|| "sfsh".to_string()),
                    ),
                    "BASH_LINENO" => Some("1".to_string()),
                    "FUNCNAME" => Some("main".to_string()),
                    _ => None,
                }
            }
            "EUID" => Some(unsafe { libc::geteuid() }.to_string()),
            "UID" => Some(unsafe { libc::getuid() }.to_string()),
            _ => {
                for frame in self.local_stack.iter().rev() {
                    if let Some(v) = frame.get(name) {
                        return Some(v.clone());
                    }
                }

                self.vars
                    .get(name)
                    .or_else(|| self.exported.get(name))
                    .cloned()
            }
        }
    }

    pub fn set(&mut self, name: &str, val: &str, export: bool) {
        if self.readonly.contains(name) {
            eprintln!("sfsh: {}: readonly variable", name);
            return;
        }

        self.set_force(name, val, export);
    }

    pub fn set_force(&mut self, name: &str, val: &str, export: bool) {
        if export {
            self.vars.remove(name);
            self.exported.insert(name.to_string(), val.to_string());
            env::set_var(name, val);
        } else {
            for frame in self.local_stack.iter_mut().rev() {
                if frame.contains_key(name) {
                    frame.insert(name.to_string(), val.to_string());
                    return;
                }
            }

            if self.exported.contains_key(name) {
                self.exported.insert(name.to_string(), val.to_string());
                env::set_var(name, val);
            } else {
                self.vars.insert(name.to_string(), val.to_string());
            }
        }
    }

    pub fn is_readonly(&self, name: &str) -> bool {
        self.readonly.contains(name)
    }

    pub fn mark_readonly(&mut self, name: &str) {
        self.readonly.insert(name.to_string());

        if self.get(name).is_none() {
            self.set_force(name, "", false);
        }
    }

    pub fn unset(&mut self, name: &str) {
        if self.readonly.contains(name) {
            eprintln!("sfsh: {}: readonly variable", name);
            return;
        }

        for frame in self.local_stack.iter_mut().rev() {
            frame.remove(name);
        }

        self.vars.remove(name);
        self.exported.remove(name);
        env::remove_var(name);
    }

    pub fn export(&mut self, name: &str) {
        if let Some(v) = self.vars.remove(name) {
            self.exported.insert(name.to_string(), v.clone());
            env::set_var(name, &v);
        } else if !self.exported.contains_key(name) {
            self.exported.insert(name.to_string(), String::new());
            env::set_var(name, "");
        }
    }

    pub fn set_positional(&mut self, args: Vec<String>) {
        self.positional = args;
    }

    pub fn set_opt(&mut self, c: char, val: bool) {
        self.opts.insert(c, val);
    }

    pub fn push_local(&mut self) {
        self.local_stack.push(HashMap::new());
    }

    pub fn pop_local(&mut self) {
        self.local_stack.pop();
    }

    pub fn local_set(&mut self, name: &str, val: &str) {
        if let Some(frame) = self.local_stack.last_mut() {
            frame.insert(name.to_string(), val.to_string());
        } else {
            self.vars.insert(name.to_string(), val.to_string());
        }
    }
}
