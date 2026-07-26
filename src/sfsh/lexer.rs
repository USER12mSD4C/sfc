use crate::sfsh::ast::{ParamOp, Word, WordPart};

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Word(Word),
    IoNumber(u32),
    Op(String),
    Newline,
    Eof,
}

pub fn lex(input: &str) -> Vec<Token> {
    let mut t = Vec::new();
    let mut cs = input.chars().peekable();
    while let Some(&c) = cs.peek() {
        if c == '\n' {
            cs.next();
            t.push(Token::Newline);
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
            t.push(Token::Op(op));
            continue;
        }
        let w = read_word(&mut cs);
        if !w.is_empty() {
            if let Ok(n) = w.parse::<u32>() {
                if let Some(Token::Op(ref s)) = t.last() {
                    if is_redir_op(s) {
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
    matches!(s, "<" | ">" | ">>" | "<<-" | "<<" | "<&" | ">&" | "<>")
}

fn read_op(cs: &mut std::iter::Peekable<std::str::Chars>) -> Option<String> {
    let ops = [
        "&&", "||", ";;", "<<-", ">>", "<<", "<&", ">&", "<>", "&", "|", ";", "(", ")", "<", ">",
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
        if read_op(&mut cs.clone()).is_some() && !s.is_empty() {
            break;
        }
        if c == '\\' {
            cs.next();
            if let Some(x) = cs.next() {
                s.push('\\');
                s.push(x);
            }
        } else if c == '\'' {
            cs.next();
            while let Some(&x) = cs.peek() {
                if x == '\'' {
                    cs.next();
                    break;
                }
                s.push(x);
                cs.next();
            }
        } else if c == '"' {
            cs.next();
            s.push('"');
            while let Some(&x) = cs.peek() {
                if x == '\\' {
                    cs.next();
                    s.push('\\');
                    if let Some(y) = cs.next() {
                        s.push(y);
                    }
                } else if x == '"' {
                    cs.next();
                    s.push('"');
                    break;
                } else {
                    s.push(x);
                    cs.next();
                }
            }
        } else if c == '`' {
            cs.next();
            while let Some(&x) = cs.peek() {
                if x == '`' {
                    cs.next();
                    break;
                }
                s.push(x);
                cs.next();
            }
        } else {
            s.push(c);
            cs.next();
        }
    }
    s
}

fn parse_word(s: &str) -> Word {
    let mut parts = Vec::new();
    let mut cs = s.chars().peekable();
    while let Some(&c) = cs.peek() {
        if c == '\'' {
            parts.push(read_squote(&mut cs));
        } else if c == '"' {
            parts.push(read_dquote(&mut cs));
        } else if c == '~' {
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
                parts.push(WordPart::Lit(x.to_string()));
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
        while let Some(&c) = cs.peek() {
            if c == '}' {
                cs.next();
                break;
            }
            name.push(c);
            cs.next();
        }
        let op = parse_param_op(&name);
        return WordPart::Param(name, op);
    }
    // Special single-char params: $? $$ $! $# $- $0 $1..$9
    if let Some(&c) = cs.peek() {
        if "?$!#-".contains(c) || c.is_ascii_digit() {
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

fn parse_param_op(s: &str) -> Option<ParamOp> {
    let mut cs = s.chars().peekable();
    let mut name = String::new();
    while let Some(&c) = cs.peek() {
        if ":-=?+#%".contains(c) {
            break;
        }
        name.push(c);
        cs.next();
    }
    let op_char = cs.peek().copied();
    let rest: String = cs.collect();
    match op_char {
        Some(':') if rest.starts_with("-") => Some(ParamOp::Def(rest[1..].to_string(), true)),
        Some(':') if rest.starts_with("=") => Some(ParamOp::Assign(rest[1..].to_string(), true)),
        Some(':') if rest.starts_with("?") => Some(ParamOp::Err(rest[1..].to_string(), true)),
        Some(':') if rest.starts_with("+") => Some(ParamOp::Alt(rest[1..].to_string(), true)),
        Some('-') => Some(ParamOp::Def(rest.to_string(), false)),
        Some('=') => Some(ParamOp::Assign(rest.to_string(), false)),
        Some('?') => Some(ParamOp::Err(rest.to_string(), false)),
        Some('+') => Some(ParamOp::Alt(rest.to_string(), false)),
        Some('#') if rest.starts_with("#") => Some(ParamOp::RemLF(rest[1..].to_string())),
        Some('#') => Some(ParamOp::RemSF(rest.to_string())),
        Some('%') if rest.starts_with("%") => Some(ParamOp::RemLB(rest[1..].to_string())),
        Some('%') => Some(ParamOp::RemSB(rest.to_string())),
        _ => None,
    }
}
