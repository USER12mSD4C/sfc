use crate::sfsh::ast::*;
use crate::sfsh::exec::execute_script_capture;
use crate::sfsh::vars::ShellVars;
use std::collections::HashMap;

pub fn expand_word(
    w: &Word,
    vars: &mut ShellVars,
    aliases: &HashMap<String, String>,
    funcs: &HashMap<String, Command>,
) -> Vec<String> {
    let ifs = vars.get("IFS").unwrap_or_else(|| " \t\n".to_string());
    let mut fields: Vec<(String, bool)> = vec![(String::new(), false)];

    for p in &w.0 {
        match p {
            WordPart::SQuote(s) => {
                let last = fields.last_mut().unwrap();
                last.0.push_str(s);
                last.1 = true;
            }
            WordPart::DQuote(parts) => {
                let generated = expand_dquote_fields(parts, vars, aliases, funcs);

                if !generated.is_empty() {
                    let mut iter = generated.into_iter();

                    if let Some(first) = iter.next() {
                        let last = fields.last_mut().unwrap();
                        last.0.push_str(&first);
                        last.1 = true;
                    }

                    for extra in iter {
                        fields.push((extra, true));
                    }
                }
            }
            WordPart::Lit(_) | WordPart::Tilde(_) | WordPart::Arith(_) => {
                let mut s = String::new();
                expand_part(p, vars, &mut s, aliases, funcs);
                fields.last_mut().unwrap().0.push_str(&s);
            }
            WordPart::Param(_, _) | WordPart::Cmd(_) => {
                let mut s = String::new();
                expand_part(p, vars, &mut s, aliases, funcs);
                split_into_fields_quoted(&s, &ifs, &mut fields);
            }
        }
    }

    let mut result = Vec::new();
    let noglob = vars.opts.get(&'f').copied().unwrap_or(false);

    for (text, quoted) in fields {
        if text.is_empty() && !quoted {
            continue;
        }

        if quoted || noglob {
            result.push(text);
        } else {
            result.extend(glob_word(&text));
        }
    }

    result
}

pub fn expand_assignment(
    w: &Word,
    vars: &mut ShellVars,
    aliases: &HashMap<String, String>,
    funcs: &HashMap<String, Command>,
) -> String {
    let mut out = String::new();

    for p in &w.0 {
        expand_part(p, vars, &mut out, aliases, funcs);
    }

    out
}

fn split_into_fields_quoted(s: &str, ifs: &str, fields: &mut Vec<(String, bool)>) {
    if ifs.is_empty() {
        fields.last_mut().unwrap().0.push_str(s);
        return;
    }

    let mut prev_was_ifs_ws = false;

    for c in s.chars() {
        if ifs.contains(c) {
            let is_ws = c == ' ' || c == '\t' || c == '\n';

            if is_ws && prev_was_ifs_ws {
                continue;
            }

            if !fields.last().unwrap().0.is_empty() {
                fields.push((String::new(), false));
            }

            prev_was_ifs_ws = is_ws;
        } else {
            fields.last_mut().unwrap().0.push(c);
            prev_was_ifs_ws = false;
        }
    }
}

fn expand_dquote_fields(
    parts: &[WordPart],
    vars: &mut ShellVars,
    aliases: &HashMap<String, String>,
    funcs: &HashMap<String, Command>,
) -> Vec<String> {
    if parts.len() == 1 {
        if let WordPart::Param(name, None) = &parts[0] {
            if name == "@" {
                return vars.positional.clone();
            }

            if name == "*" {
                if vars.positional.is_empty() {
                    return vec![String::new()];
                }

                let sep = vars
                    .get("IFS")
                    .unwrap_or_else(|| " \t\n".to_string())
                    .chars()
                    .next()
                    .unwrap_or(' ')
                    .to_string();

                return vec![vars.positional.join(&sep)];
            }
        }
    }

    let mut fields: Vec<String> = vec![String::new()];

    for p in parts {
        match p {
            WordPart::Param(name, None) if name == "@" => {
                let vals = vars.positional.clone();

                if vals.is_empty() {
                    continue;
                }

                let mut iter = vals.into_iter();

                if let Some(first) = iter.next() {
                    fields.last_mut().unwrap().push_str(&first);
                }

                for val in iter {
                    fields.push(val);
                }
            }
            WordPart::Param(name, None) if name == "*" => {
                let sep = vars
                    .get("IFS")
                    .unwrap_or_else(|| " \t\n".to_string())
                    .chars()
                    .next()
                    .unwrap_or(' ')
                    .to_string();

                let joined = vars.positional.join(&sep);
                fields.last_mut().unwrap().push_str(&joined);
            }
            WordPart::DQuote(inner) => {
                let inner_fields = expand_dquote_fields(inner, vars, aliases, funcs);
                let mut iter = inner_fields.into_iter();

                if let Some(first) = iter.next() {
                    fields.last_mut().unwrap().push_str(&first);
                }

                for extra in iter {
                    fields.push(extra);
                }
            }
            _ => {
                let mut s = String::new();
                expand_part(p, vars, &mut s, aliases, funcs);
                fields.last_mut().unwrap().push_str(&s);
            }
        }
    }

    fields
}

fn glob_word(s: &str) -> Vec<String> {
    if !s.contains('*') && !s.contains('?') && !s.contains('[') {
        return vec![s.to_string()];
    }
    let (dir, pat, prefix) = if let Some(pos) = s.rfind('/') {
        let d = if pos == 0 { "/" } else { &s[..pos] };
        (d, &s[pos + 1..], format!("{}/", &s[..pos]))
    } else {
        (".", s, String::new())
    };
    let mut res = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') && !pat.starts_with('.') {
                continue;
            }
            if match_glob(pat, &name) {
                let full = if prefix.is_empty() {
                    name
                } else if prefix == "/" {
                    format!("/{}", name)
                } else {
                    format!("{}{}", prefix, name)
                };
                res.push(full);
            }
        }
    }
    if res.is_empty() {
        vec![s.to_string()]
    } else {
        res.sort();
        res
    }
}

pub fn match_glob(pat: &str, s: &str) -> bool {
    let pc: Vec<char> = pat.chars().collect();
    let sc: Vec<char> = s.chars().collect();
    match_glob_chars(&pc, &sc)
}

fn match_glob_chars(pc: &[char], sc: &[char]) -> bool {
    let mut pi = 0;
    let mut si = 0;
    while pi < pc.len() {
        if pc[pi] == '*' {
            pi += 1;
            if pi == pc.len() {
                return true;
            }
            for i in si..=sc.len() {
                if match_glob_chars(&pc[pi..], &sc[i..]) {
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
            let mut end = pi + 1;
            let negate = end < pc.len() && (pc[end] == '!' || pc[end] == '^');
            if negate {
                end += 1;
            }
            if end < pc.len() && pc[end] == ']' {
                end += 1;
            }
            while end < pc.len() && pc[end] != ']' {
                end += 1;
            }
            if end >= pc.len() {
                if pc[pi] != sc[si] {
                    return false;
                }
                pi += 1;
                si += 1;
                continue;
            }
            let class = &pc[if negate { pi + 2 } else { pi + 1 }..end];
            let matched = match_char_class(class, sc[si]);
            if negate == matched {
                return false;
            }
            pi = end + 1;
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

fn match_char_class(class: &[char], c: char) -> bool {
    let mut i = 0;
    while i < class.len() {
        if i + 2 < class.len() && class[i + 1] == '-' {
            if c >= class[i] && c <= class[i + 2] {
                return true;
            }
            i += 3;
        } else {
            if class[i] == c {
                return true;
            }
            i += 1;
        }
    }
    false
}

fn expand_part(
    p: &WordPart,
    vars: &mut ShellVars,
    out: &mut String,
    aliases: &HashMap<String, String>,
    funcs: &HashMap<String, Command>,
) {
    match p {
        WordPart::Lit(s) | WordPart::SQuote(s) => out.push_str(s),
        WordPart::Tilde(ref s) => {
            if s.is_empty() {
                out.push_str(&std::env::var("HOME").unwrap_or_default());
            } else {
                if let Some(home) = lookup_user_home(s) {
                    out.push_str(&home);
                } else {
                    out.push('~');
                    out.push_str(s);
                }
            }
        }
        WordPart::Param(name, op) => {
            if name == "@" || name == "*" {
                let vals: Vec<String> = vars.positional.clone();
                let sep = if name == "*" {
                    vars.get("IFS")
                        .unwrap_or_else(|| " \t\n".to_string())
                        .chars()
                        .next()
                        .unwrap_or(' ')
                        .to_string()
                } else {
                    " ".to_string()
                };
                out.push_str(&vals.join(&sep));
                return;
            }

            let val_opt = vars.get(name);
            let unset = val_opt.is_none();
            let mut val = val_opt.unwrap_or_default();

            if let Some(ref o) = op {
                match o {
                    ParamOp::Def(def, col) => {
                        let col = *col;
                        if (col && val.is_empty()) || (!col && unset) {
                            val = def.clone();
                        }
                    }
                    ParamOp::Assign(def, col) => {
                        let col = *col;
                        if (col && val.is_empty()) || (!col && unset) {
                            val = def.clone();
                            vars.set(name, &val, false);
                        }
                    }
                    ParamOp::Err(msg, col) => {
                        let col = *col;
                        if (col && val.is_empty()) || (!col && unset) {
                            eprintln!("sfsh: {}: {}", name, msg);
                            val = String::new();
                        }
                    }
                    ParamOp::Alt(alt, col) => {
                        let col = *col;
                        if col {
                            if !val.is_empty() {
                                val = alt.clone();
                            } else {
                                val = String::new();
                            }
                        } else {
                            if !unset {
                                val = alt.clone();
                            } else {
                                val = String::new();
                            }
                        }
                    }
                    ParamOp::Len => {
                        val = vars.get(name).unwrap_or_default().len().to_string();
                    }
                    ParamOp::Off(off) => {
                        let v = vars.get(name).unwrap_or_default();
                        let start: usize = off.parse().unwrap_or(0);
                        val = v.chars().skip(start).collect();
                    }
                    ParamOp::OffLen(off, len) => {
                        let v = vars.get(name).unwrap_or_default();
                        let start: usize = off.parse().unwrap_or(0);
                        let length: usize = len.parse().unwrap_or(0);
                        val = v.chars().skip(start).take(length).collect();
                    }
                    ParamOp::RemSF(pat) => {
                        let v = vars.get(name).unwrap_or_default();
                        val = remove_prefix(&v, pat, false);
                    }
                    ParamOp::RemLF(pat) => {
                        let v = vars.get(name).unwrap_or_default();
                        val = remove_prefix(&v, pat, true);
                    }
                    ParamOp::RemSB(pat) => {
                        let v = vars.get(name).unwrap_or_default();
                        val = remove_suffix(&v, pat, false);
                    }
                    ParamOp::RemLB(pat) => {
                        let v = vars.get(name).unwrap_or_default();
                        val = remove_suffix(&v, pat, true);
                    }
                }
            }
            out.push_str(&val);
        }
        WordPart::Cmd(cmd) => match execute_script_capture(cmd, vars, aliases, funcs) {
            Ok(mut buf) => {
                while buf.ends_with('\n') {
                    buf.pop();
                }
                out.push_str(&buf);
            }
            Err(_) => {}
        },
        WordPart::Arith(expr) => {
            out.push_str(&eval_arith(expr, vars));
        }
        WordPart::DQuote(parts) => {
            for p in parts {
                expand_part(p, vars, out, aliases, funcs);
            }
        }
    }
}

fn remove_prefix(val: &str, pat: &str, longest: bool) -> String {
    let vc: Vec<char> = val.chars().collect();
    let pc: Vec<char> = pat.chars().collect();
    let mut best = 0;
    let mut found = false;
    for end in 1..=vc.len() {
        if match_glob_chars(&pc, &vc[..end]) {
            if !longest {
                return vc[end..].iter().collect();
            }
            best = end;
            found = true;
        }
    }
    if found {
        vc[best..].iter().collect()
    } else {
        val.to_string()
    }
}

fn remove_suffix(val: &str, pat: &str, longest: bool) -> String {
    let vc: Vec<char> = val.chars().collect();
    let pc: Vec<char> = pat.chars().collect();
    let mut best = None;

    for start in 0..=vc.len() {
        if match_glob_chars(&pc, &vc[start..]) {
            if longest {
                return vc[..start].iter().collect();
            }

            best = Some(start);
        }
    }

    match best {
        Some(start) => vc[..start].iter().collect(),
        None => val.to_string(),
    }
}

fn lookup_user_home(user: &str) -> Option<String> {
    let passwd = std::fs::read_to_string("/etc/passwd").ok()?;
    for line in passwd.lines() {
        let fields: Vec<&str> = line.split(':').collect();
        if fields.len() >= 6 && fields[0] == user {
            return Some(fields[5].to_string());
        }
    }
    None
}

fn eval_arith(expr: &str, vars: &ShellVars) -> String {
    let mut expanded = String::new();
    let mut chars = expr.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_alphabetic() || c == '_' {
            let mut name = String::new();
            while let Some(&c2) = chars.peek() {
                if c2.is_alphanumeric() || c2 == '_' {
                    name.push(c2);
                    chars.next();
                } else {
                    break;
                }
            }
            let val = vars.get(&name).unwrap_or_default();
            expanded.push_str(&val);
        } else {
            expanded.push(c);
            chars.next();
        }
    }
    match eval_expr(&expanded) {
        Ok(n) => n.to_string(),
        Err(_) => "0".to_string(),
    }
}

#[derive(Debug, Clone, Copy)]
enum ArithToken {
    Num(i64),
    Op(char),
    LParen,
    RParen,
}

fn tokenize_arith(s: &str) -> Result<Vec<ArithToken>, ()> {
    let mut tokens = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        if c.is_ascii_digit() {
            let mut num = 0i64;
            while let Some(&c2) = chars.peek() {
                if c2.is_ascii_digit() {
                    num = num * 10 + (c2 as i64 - '0' as i64);
                    chars.next();
                } else {
                    break;
                }
            }
            tokens.push(ArithToken::Num(num));
        } else if c == '+' || c == '-' || c == '*' || c == '/' || c == '%' || c == '(' || c == ')' {
            tokens.push(if c == '(' {
                ArithToken::LParen
            } else if c == ')' {
                ArithToken::RParen
            } else {
                ArithToken::Op(c)
            });
            chars.next();
        } else {
            return Err(());
        }
    }
    Ok(tokens)
}

fn eval_expr(s: &str) -> Result<i64, ()> {
    let tokens = tokenize_arith(s)?;
    let mut pos = 0;
    parse_expr(&tokens, &mut pos)
}

fn parse_expr(tokens: &[ArithToken], pos: &mut usize) -> Result<i64, ()> {
    let mut left = parse_term(tokens, pos)?;
    while *pos < tokens.len() {
        match tokens[*pos] {
            ArithToken::Op('+') => {
                *pos += 1;
                left = left.checked_add(parse_term(tokens, pos)?).ok_or(())?;
            }
            ArithToken::Op('-') => {
                *pos += 1;
                left = left.checked_sub(parse_term(tokens, pos)?).ok_or(())?;
            }
            _ => break,
        }
    }
    Ok(left)
}

fn parse_term(tokens: &[ArithToken], pos: &mut usize) -> Result<i64, ()> {
    let mut left = parse_factor(tokens, pos)?;
    while *pos < tokens.len() {
        match tokens[*pos] {
            ArithToken::Op('*') => {
                *pos += 1;
                left = left.checked_mul(parse_factor(tokens, pos)?).ok_or(())?;
            }
            ArithToken::Op('/') => {
                *pos += 1;
                let right = parse_factor(tokens, pos)?;
                if right == 0 {
                    return Err(());
                }
                left = left / right;
            }
            ArithToken::Op('%') => {
                *pos += 1;
                let right = parse_factor(tokens, pos)?;
                if right == 0 {
                    return Err(());
                }
                left = left % right;
            }
            _ => break,
        }
    }
    Ok(left)
}

fn parse_factor(tokens: &[ArithToken], pos: &mut usize) -> Result<i64, ()> {
    if *pos >= tokens.len() {
        return Err(());
    }
    match tokens[*pos] {
        ArithToken::Num(n) => {
            *pos += 1;
            Ok(n)
        }
        ArithToken::Op('+') => {
            *pos += 1;
            parse_factor(tokens, pos)
        }
        ArithToken::Op('-') => {
            *pos += 1;
            Ok(-parse_factor(tokens, pos)?)
        }
        ArithToken::LParen => {
            *pos += 1;
            let val = parse_expr(tokens, pos)?;
            if *pos >= tokens.len() || !matches!(tokens[*pos], ArithToken::RParen) {
                return Err(());
            }
            *pos += 1;
            Ok(val)
        }
        _ => Err(()),
    }
}
