use crate::sfsh::ast::*;
use crate::sfsh::vars::ShellVars;
use std::io::Read;
use std::process::Command as ProcCmd;

pub fn expand_word(w: &Word, vars: &ShellVars) -> Vec<String> {
    let mut s = String::new();
    for p in &w.0 {
        expand_part(p, vars, &mut s);
    }
    let ifs = vars.get("IFS").unwrap_or_else(|| " \t\n".to_string());
    let fields = split_fields(&s, &ifs);
    fields.into_iter().flat_map(|f| glob_word(&f)).collect()
}

fn split_fields(s: &str, ifs: &str) -> Vec<String> {
    if ifs.is_empty() {
        return vec![s.to_string()];
    }
    let mut res = Vec::new();
    let mut cur = String::new();
    for c in s.chars() {
        if ifs.contains(c) {
            if !cur.is_empty() {
                res.push(cur);
                cur = String::new();
            }
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        res.push(cur);
    }
    res
}

fn glob_word(s: &str) -> Vec<String> {
    if !s.contains('*') && !s.contains('?') && !s.contains('[') {
        return vec![s.to_string()];
    }
    let mut res = Vec::new();
    if let Ok(entries) = std::fs::read_dir(".") {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if match_glob(s, &name) {
                res.push(name);
            }
        }
    }
    if res.is_empty() {
        vec![s.to_string()]
    } else {
        res
    }
}

pub fn match_glob(pat: &str, s: &str) -> bool {
    // Simple glob: *, ?, [abc]
    let mut pi = 0;
    let mut si = 0;
    let pc: Vec<char> = pat.chars().collect();
    let sc: Vec<char> = s.chars().collect();
    while pi < pc.len() {
        if pc[pi] == '*' {
            pi += 1;
            if pi == pc.len() {
                return true;
            }
            for i in si..=sc.len() {
                if match_glob(&pat[pi..], &s[i..]) {
                    return true;
                }
            }
            return false;
        }
        if si >= sc.len() {
            return false;
        }
        if pc[pi] == '?' {
            pi += 1;
            si += 1;
            continue;
        }
        if pc[pi] == '[' {
            let end = pc[pi..].iter().position(|&c| c == ']').unwrap_or(pc.len());
            let set: String = pc[pi + 1..pi + end].iter().collect();
            if !set.contains(sc[si]) {
                return false;
            }
            pi += end + 1;
            si += 1;
            continue;
        }
        if pc[pi] != sc[si] {
            return false;
        }
        pi += 1;
        si += 1;
    }
    si == sc.len()
}

fn expand_part(p: &WordPart, vars: &ShellVars, out: &mut String) {
    match p {
        WordPart::Lit(s) | WordPart::SQuote(s) => out.push_str(s),
        WordPart::Tilde(ref s) => {
            if s.is_empty() {
                out.push_str(&std::env::var("HOME").unwrap_or_default());
            } else {
                out.push('~');
                out.push_str(s);
            }
        }
        WordPart::Param(name, op) => {
            let mut val = vars.get(name).unwrap_or_default();
            if let Some(ref o) = op {
                match o {
                    ParamOp::Def(def, col) => {
                        if val.is_empty() || *col {
                            val = def.clone();
                        }
                    }
                    ParamOp::Assign(def, col) => {
                        if val.is_empty() || *col {
                            val = def.clone();
                        }
                    }
                    ParamOp::Err(msg, _) => {
                        if val.is_empty() {
                            eprintln!("{}: {}", name, msg);
                            val = String::new();
                        }
                    }
                    ParamOp::Alt(alt, col) => {
                        if !val.is_empty() || !col {
                            val = alt.clone();
                        }
                    }
                    ParamOp::Len => val = name.len().to_string(),
                    _ => {}
                }
            }
            out.push_str(&val);
        }
        WordPart::Cmd(cmd) => {
            let mut child = ProcCmd::new("sh")
                .arg("-c")
                .arg(cmd)
                .stdout(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            if let Some(mut o) = child.stdout.take() {
                let mut buf = String::new();
                o.read_to_string(&mut buf).unwrap();
                out.push_str(buf.trim_end_matches('\n'));
            }
        }
        WordPart::Arith(expr) => {
            if let Ok(n) = expr.parse::<i64>() {
                out.push_str(&n.to_string());
            } else {
                out.push_str(expr);
            }
        }
        WordPart::DQuote(parts) => {
            for p in parts {
                expand_part(p, vars, out);
            }
        }
    }
}
