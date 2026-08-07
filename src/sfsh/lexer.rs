use crate::sfsh::ast::{ParamOp, Word, WordPart};

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Word(Word),
    IoNumber(u32),
    Op(String),
    HereBody(String),
    Newline,
    Eof,
}

pub fn lex(input: &str) -> Vec<Token> {
    let mut t = Vec::new();
    let mut cs = input.chars().peekable();
    let mut pending_heredocs: Vec<(bool, String)> = Vec::new();
    let mut expect_heredoc: Option<bool> = None;

    while let Some(&c) = cs.peek() {
        if c == '\n' {
            cs.next();
            if pending_heredocs.is_empty() {
                t.push(Token::Newline);
            } else {
                let pendings = std::mem::take(&mut pending_heredocs);
                for (strip, delim) in pendings {
                    let body = read_heredoc_body(&mut cs, &delim, strip);
                    t.push(Token::HereBody(body));
                }
                t.push(Token::Newline);
            }
            continue;
        }

        if c.is_whitespace() {
            cs.next();
            continue;
        }

        if c == '#' {
            while let Some(&x) = cs.peek() {
                if x == '\n' {
                    break;
                }
                cs.next();
            }
            continue;
        }

        if let Some(op) = read_op(&mut cs) {
            if op == "<<" {
                expect_heredoc = Some(false);
            } else if op == "<<-" {
                expect_heredoc = Some(true);
            }
            t.push(Token::Op(op));
            continue;
        }

        let w = read_word(&mut cs);
        if !w.is_empty() {
            if let Some(strip) = expect_heredoc.take() {
                let (delim, _quoted) = parse_heredoc_delim(&w);
                pending_heredocs.push((strip, delim));
            }

            if let Ok(n) = w.parse::<u32>() {
                let mut clone = cs.clone();
                if let Some(op) = read_op(&mut clone) {
                    if is_redir_op(&op) {
                        t.push(Token::IoNumber(n));
                        continue;
                    }
                }
            }
            t.push(Token::Word(parse_word(&w)));
        }
    }
    t.push(Token::Eof);
    t
}

fn is_redir_op(s: &str) -> bool {
    matches!(s, "<" | ">" | ">>" | "<<-" | "<<" | "<&" | ">&" | "<>" | "&>")
}

fn read_op(cs: &mut std::iter::Peekable<std::str::Chars>) -> Option<String> {
    let ops = [
        "&&", "||", ";;", "<<-", ">>", "<<", "<&", ">&", "<>", "&>", "|&", "&", "|", ";", "(", ")", "<", ">",
    ];
    let rest: String = cs.clone().take(4).collect();
    for op in ops {
        if rest.starts_with(op) {
            for _ in 0..op.len() {
                cs.next();
            }
            return Some(op.to_string());
        }
    }
    None
}

fn read_word(cs: &mut std::iter::Peekable<std::str::Chars>) -> String {
    let mut s = String::new();
    while let Some(&c) = cs.peek() {
        if c.is_whitespace() || c == '\n' {
            break;
        }
        if !s.is_empty() {
            let mut clone = cs.clone();
            if read_op(&mut clone).is_some() {
                break;
            }
        }

        if c == '\\' {
            cs.next();
            if let Some(x) = cs.next() {
                s.push('\\');
                s.push(x);
            }
        } else if c == '\'' {
            s.push_str(&read_raw_squote(cs));
        } else if c == '"' {
            s.push_str(&read_raw_dquote(cs));
        } else if c == '`' {
            cs.next();
            s.push_str("$(");
            while let Some(&x) = cs.peek() {
                if x == '\\' {
                    cs.next();
                    if let Some(y) = cs.next() {
                        s.push('\\');
                        s.push(y);
                    }
                } else if x == '`' {
                    cs.next();
                    break;
                } else {
                    s.push(x);
                    cs.next();
                }
            }
            s.push(')');
        } else if c == '$' {
            cs.next();
            s.push('$');
            if let Some(&'(') = cs.peek() {
                s.push_str(&read_dollar_paren(cs));
            } else if let Some(&'{') = cs.peek() {
                s.push_str(&read_dollar_brace(cs));
            }
        } else {
            s.push(c);
            cs.next();
        }
    }
    s
}

fn read_raw_squote(cs: &mut std::iter::Peekable<std::str::Chars>) -> String {
    cs.next();
    let mut s = String::from("'");
    while let Some(&c) = cs.peek() {
        if c == '\'' {
            cs.next();
            s.push('\'');
            break;
        }
        s.push(c);
        cs.next();
    }
    s
}

fn read_raw_dquote(cs: &mut std::iter::Peekable<std::str::Chars>) -> String {
    cs.next();
    let mut s = String::from("\"");
    while let Some(&c) = cs.peek() {
        if c == '\\' {
            cs.next();
            s.push('\\');
            if let Some(&x) = cs.peek() {
                cs.next();
                s.push(x);
            }
        } else if c == '"' {
            cs.next();
            s.push('"');
            break;
        } else {
            s.push(c);
            cs.next();
        }
    }
    s
}

fn read_dollar_paren(cs: &mut std::iter::Peekable<std::str::Chars>) -> String {
    cs.next();
    let mut out = String::from("(");
    if let Some(&'(') = cs.peek() {
        cs.next();
        out.push('(');
        let mut depth = 2;
        while let Some(&c) = cs.peek() {
            cs.next();
            out.push(c);
            if c == '(' {
                depth += 1;
            } else if c == ')' {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
        }
        return out;
    }

    let mut depth = 1;
    while let Some(&c) = cs.peek() {
        match c {
            '\'' => {
                out.push_str(&read_raw_squote(cs));
            }
            '"' => {
                out.push_str(&read_raw_dquote(cs));
            }
            '\\' => {
                cs.next();
                out.push('\\');
                if let Some(&x) = cs.peek() {
                    cs.next();
                    out.push(x);
                }
            }
            '$' => {
                cs.next();
                out.push('$');
                if let Some(&'(') = cs.peek() {
                    out.push_str(&read_dollar_paren(cs));
                } else if let Some(&'{') = cs.peek() {
                    out.push_str(&read_dollar_brace(cs));
                }
            }
            '(' => {
                cs.next();
                out.push('(');
                depth += 1;
            }
            ')' => {
                cs.next();
                out.push(')');
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {
                cs.next();
                out.push(c);
            }
        }
    }
    out
}

fn read_dollar_brace(cs: &mut std::iter::Peekable<std::str::Chars>) -> String {
    cs.next();
    let mut out = String::from("{");
    let mut depth = 1;
    while let Some(&c) = cs.peek() {
        match c {
            '\'' => {
                out.push_str(&read_raw_squote(cs));
            }
            '"' => {
                out.push_str(&read_raw_dquote(cs));
            }
            '\\' => {
                cs.next();
                out.push('\\');
                if let Some(&x) = cs.peek() {
                    cs.next();
                    out.push(x);
                }
            }
            '$' => {
                cs.next();
                out.push('$');
                if let Some(&'(') = cs.peek() {
                    out.push_str(&read_dollar_paren(cs));
                } else if let Some(&'{') = cs.peek() {
                    out.push_str(&read_dollar_brace(cs));
                }
            }
            '{' => {
                cs.next();
                out.push('{');
                depth += 1;
            }
            '}' => {
                cs.next();
                out.push('}');
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {
                cs.next();
                out.push(c);
            }
        }
    }
    out
}

pub fn parse_word(s: &str) -> Word {
    let mut parts = Vec::new();
    let mut cs = s.chars().peekable();
    while let Some(&c) = cs.peek() {
        if c == '\'' {
            parts.push(read_squote(&mut cs));
        } else if c == '"' {
            parts.push(read_dquote(&mut cs));
        } else if c == '~' && parts.is_empty() {
            parts.push(read_tilde(&mut cs));
        } else if c == '$' {
            parts.push(read_dollar(&mut cs));
        } else {
            parts.push(read_lit(&mut cs));
        }
    }
    Word(parts)
}

fn read_squote(cs: &mut std::iter::Peekable<std::str::Chars>) -> WordPart {
    cs.next();
    let mut s = String::new();
    while let Some(&c) = cs.peek() {
        if c == '\'' {
            cs.next();
            break;
        }
        s.push(c);
        cs.next();
    }
    WordPart::SQuote(s)
}

fn read_dquote(cs: &mut std::iter::Peekable<std::str::Chars>) -> WordPart {
    cs.next();
    let mut parts = Vec::new();
    while let Some(&c) = cs.peek() {
        if c == '"' {
            cs.next();
            break;
        }
        if c == '\\' {
            cs.next();
            if let Some(x) = cs.next() {
                match x {
                    '"' | '\\' | '`' | '$' | '\n' => {
                        if x != '\n' {
                            parts.push(WordPart::Lit(x.to_string()));
                        }
                    }
                    _ => {
                        parts.push(WordPart::Lit('\\'.to_string()));
                        parts.push(WordPart::Lit(x.to_string()));
                    }
                }
            }
        } else if c == '$' {
            parts.push(read_dollar(cs));
        } else if c == '`' {
            cs.next();
            let mut s = String::new();
            while let Some(&x) = cs.peek() {
                if x == '`' {
                    cs.next();
                    break;
                }
                s.push(x);
                cs.next();
            }
            parts.push(WordPart::Cmd(s));
        } else {
            let x = cs.next().unwrap();
            parts.push(WordPart::Lit(x.to_string()));
        }
    }
    WordPart::DQuote(parts)
}

fn read_tilde(cs: &mut std::iter::Peekable<std::str::Chars>) -> WordPart {
    cs.next();
    let mut s = String::new();
    while let Some(&c) = cs.peek() {
        if c.is_whitespace() || c == '/' || c == ':' {
            break;
        }
        s.push(c);
        cs.next();
    }
    WordPart::Tilde(s)
}

fn read_dollar(cs: &mut std::iter::Peekable<std::str::Chars>) -> WordPart {
    cs.next();
    if let Some(&'(') = cs.peek() {
        cs.next();
        if let Some(&'(') = cs.peek() {
            cs.next();
            let mut s = String::new();
            let mut d = 0;
            while let Some(&c) = cs.peek() {
                if c == '(' {
                    d += 1;
                }
                if c == ')' {
                    if d == 0 {
                        cs.next();
                        break;
                    }
                    d -= 1;
                }
                s.push(c);
                cs.next();
            }
            if let Some(&')') = cs.peek() {
                cs.next();
            }
            return WordPart::Arith(s);
        }

        let mut s = String::new();
        let mut d = 0;
        while let Some(&c) = cs.peek() {
            if c == '(' {
                d += 1;
            }
            if c == ')' {
                if d == 0 {
                    cs.next();
                    break;
                }
                d -= 1;
            }
            s.push(c);
            cs.next();
        }
        return WordPart::Cmd(s);
    }

    let mut name = String::new();
    if let Some(&'{') = cs.peek() {
        cs.next();
        if let Some(&'#') = cs.peek() {
            cs.next();
            while let Some(&c) = cs.peek() {
                if c == '}' {
                    cs.next();
                    break;
                }
                name.push(c);
                cs.next();
            }
            return WordPart::Param(name, Some(ParamOp::Len));
        }

        while let Some(&c) = cs.peek() {
            if c == '}' {
                cs.next();
                break;
            }
            name.push(c);
            cs.next();
        }
        let (name, op) = parse_param_expansion(&name);
        return WordPart::Param(name, op);
    }

    if let Some(&c) = cs.peek() {
        if "?$!#-@*".contains(c) || c.is_ascii_digit() {
            cs.next();
            name.push(c);
            return WordPart::Param(name, None);
        }
    }

    while let Some(&c) = cs.peek() {
        if !c.is_alphanumeric() && c != '_' {
            break;
        }
        name.push(c);
        cs.next();
    }
    WordPart::Param(name, None)
}

fn read_lit(cs: &mut std::iter::Peekable<std::str::Chars>) -> WordPart {
    let mut s = String::new();
    while let Some(&c) = cs.peek() {
        if c == '\'' || c == '"' || c == '~' || c == '$' || c == '`' {
            break;
        }
        if c == '\\' {
            cs.next();
            if let Some(x) = cs.next() {
                s.push(x);
            }
            continue;
        }
        s.push(c);
        cs.next();
    }
    WordPart::Lit(s)
}

fn parse_param_expansion(inner: &str) -> (String, Option<ParamOp>) {
    if let Some(name) = inner.strip_prefix('#') {
        return (name.to_string(), Some(ParamOp::Len));
    }

    let chars: Vec<char> = inner.chars().collect();
    let mut idx = 0;
    while idx < chars.len() {
        let c = chars[idx];
        if idx > 0 && matches!(c, ':' | '-' | '=' | '?' | '+' | '#' | '%') {
            break;
        }
        idx += 1;
    }

    let name: String = chars[..idx].iter().collect();
    if idx == chars.len() {
        return (name, None);
    }

    let op_char = chars[idx];
    let rest: String = chars[idx + 1..].iter().collect();

    let op = match op_char {
        ':' => {
            if let Some(op2) = rest.chars().next() {
                let tail: String = rest.chars().skip(1).collect();
                match op2 {
                    '-' => Some(ParamOp::Def(tail, true)),
                    '=' => Some(ParamOp::Assign(tail, true)),
                    '?' => Some(ParamOp::Err(tail, true)),
                    '+' => Some(ParamOp::Alt(tail, true)),
                    _ => {
                        if let Some(pos) = rest.find(':') {
                            let offset = rest[..pos].to_string();
                            let length = rest[pos + 1..].to_string();
                            Some(ParamOp::OffLen(offset, length))
                        } else {
                            Some(ParamOp::Off(rest))
                        }
                    }
                }
            } else {
                Some(ParamOp::Off(String::new()))
            }
        }
        '-' => Some(ParamOp::Def(rest, false)),
        '=' => Some(ParamOp::Assign(rest, false)),
        '?' => Some(ParamOp::Err(rest, false)),
        '+' => Some(ParamOp::Alt(rest, false)),
        '#' => {
            if let Some(tail) = rest.strip_prefix('#') {
                Some(ParamOp::RemLF(tail.to_string()))
            } else {
                Some(ParamOp::RemSF(rest))
            }
        }
        '%' => {
            if let Some(tail) = rest.strip_prefix('%') {
                Some(ParamOp::RemLB(tail.to_string()))
            } else {
                Some(ParamOp::RemSB(rest))
            }
        }
        _ => None,
    };
    (name, op)
}

fn parse_heredoc_delim(raw: &str) -> (String, bool) {
    if raw.len() >= 2 && raw.starts_with('\'') && raw.ends_with('\'') {
        return (raw[1..raw.len() - 1].to_string(), true);
    }
    if raw.len() >= 2 && raw.starts_with('"') && raw.ends_with('"') {
        return (raw[1..raw.len() - 1].to_string(), true);
    }
    (raw.to_string(), false)
}

fn read_heredoc_body(
    cs: &mut std::iter::Peekable<std::str::Chars>,
    delim: &str,
    strip: bool,
) -> String {
    let mut body = String::new();
    loop {
        let mut line = String::new();
        let mut ended = false;
        while let Some(&c) = cs.peek() {
            cs.next();
            if c == '\n' {
                ended = true;
                break;
            }
            line.push(c);
        }

        let effective = if strip {
            line.trim_start_matches('\t').to_string()
        } else {
            line.clone()
        };

        if effective == delim {
            break;
        }

        body.push_str(&effective);
        body.push('\n');

        if !ended {
            break;
        }
    }
    body
}
