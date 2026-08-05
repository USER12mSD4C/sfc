use crate::sfsh::ast::*;
use crate::sfsh::lexer::Token;

pub struct Parser {
    toks: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(toks: Vec<Token>) -> Self {
        Self { toks, pos: 0 }
    }

    fn peek(&self) -> &Token {
        self.toks.get(self.pos).unwrap_or(&Token::Eof)
    }
    fn next(&mut self) -> Token {
        let t = self.toks.get(self.pos).cloned().unwrap_or(Token::Eof);
        self.pos += 1;
        t
    }

    fn at(&self, s: &str) -> bool {
        match self.peek() {
            Token::Op(ref x) => x == s,
            Token::Word(ref w) => w.0.len() == 1 && matches!(&w.0[0], WordPart::Lit(x) if x == s),
            _ => false,
        }
    }
    fn at_word(&self) -> bool {
        matches!(self.peek(), Token::Word(_))
    }
    fn consume(&mut self, s: &str) -> bool {
        if self.at(s) {
            self.next();
            true
        } else {
            false
        }
    }

    pub fn parse(&mut self) -> Command {
        self.parse_list()
    }

    fn parse_list(&mut self) -> Command {
        let mut cmds = Vec::new();

        loop {
            while self.consume_newline() {}

            if self.at_list_terminator() {
                break;
            }

            let p = self.parse_pipeline();
            if let Command::Empty = p {
                break;
            }

            let op = if self.consume("&&") {
                Some("&&".to_string())
            } else if self.consume("||") {
                Some("||".to_string())
            } else if self.consume("&") {
                Some("&".to_string())
            } else if self.consume(";") {
                Some(";".to_string())
            } else if self.consume_newline() {
                Some(";".to_string())
            } else {
                None
            };

            let done = op.is_none();
            cmds.push((p, op));

            if done {
                break;
            }
        }

        if cmds.is_empty() {
            Command::Empty
        } else {
            Command::List(cmds)
        }
    }

    fn at_list_terminator(&self) -> bool {
        self.at(")")
            || self.at("}")
            || self.at("fi")
            || self.at("done")
            || self.at("esac")
            || self.at("else")
            || self.at("elif")
            || self.at("then")
            || self.at("do")
            || matches!(self.peek(), Token::Eof)
    }

    fn parse_pipeline(&mut self) -> Command {
        let mut cmds = Vec::new();
        let c = self.parse_command();
        if let Command::Empty = c {
            return Command::Empty;
        }
        cmds.push(c);
        while self.consume("|") {
            while self.consume_newline() {}
            let c = self.parse_command();
            if let Command::Empty = c {
                break;
            }
            cmds.push(c);
        }
        if cmds.len() == 1 {
            cmds.into_iter().next().unwrap()
        } else {
            Command::Pipeline(cmds)
        }
    }

    fn parse_command(&mut self) -> Command {
        if self.consume("(") {
            let c = self.parse_list();
            self.expect(")");
            return Command::Subshell(Box::new(c));
        }
        if self.at("{") {
            self.next();
            let c = self.parse_list();
            self.expect("}");
            return Command::Brace(Box::new(c));
        }

        if self.at_word() {
            if let Token::Word(w) = self.peek().clone() {
                let s = word_str(&w);
                match s.as_str() {
                    "if" => return self.parse_if(),
                    "for" => return self.parse_for(),
                    "while" => return self.parse_while(),
                    "until" => return self.parse_until(),
                    "case" => return self.parse_case(),
                    _ => {}
                }
                if self
                    .toks
                    .get(self.pos + 1)
                    .map(|t| matches!(t, Token::Op(ref x) if x == "("))
                    .unwrap_or(false)
                    && self
                        .toks
                        .get(self.pos + 2)
                        .map(|t| matches!(t, Token::Op(ref x) if x == ")"))
                        .unwrap_or(false)
                {
                    return self.parse_function(s);
                }
                if self
                    .toks
                    .get(self.pos + 1)
                    .map(|t| {
                        matches!(t, Token::Word(ref w2) if {
                            w2.0.len() == 1 && matches!(&w2.0[0], WordPart::Lit(x) if x == "{")
                        })
                    })
                    .unwrap_or(false)
                {
                    return self.parse_function_no_paren(s);
                }
            }
        }
        self.parse_simple()
    }

    fn parse_if(&mut self) -> Command {
        self.next();
        let cond = self.parse_list();
        self.expect("then");
        while self.consume_newline() {}
        let then_ = self.parse_list();
        let else_ = self.parse_else();
        self.expect("fi");
        Command::If(Box::new(cond), Box::new(then_), else_)
    }

    fn parse_else(&mut self) -> Option<Box<Command>> {
        if self.consume("elif") {
            let cond = self.parse_list();
            self.expect("then");
            while self.consume_newline() {}
            let then_ = self.parse_list();
            let else_ = self.parse_else();
            return Some(Box::new(Command::If(
                Box::new(cond),
                Box::new(then_),
                else_,
            )));
        }
        if self.consume("else") {
            while self.consume_newline() {}
            return Some(Box::new(self.parse_list()));
        }
        None
    }

    fn parse_for(&mut self) -> Command {
        self.next();
        let var = self.expect_word();
        while self.consume_newline() {}
        let mut words = Vec::new();
        if self.consume("in") {
            while self.at_word() && !self.at("do") {
                words.push(self.expect_word_obj());
            }
            self.consume(";");
            while self.consume_newline() {}
        }
        self.expect("do");
        let body = self.parse_list();
        self.expect("done");
        Command::For(var, words, Box::new(body))
    }

    fn parse_while(&mut self) -> Command {
        self.next();
        let cond = self.parse_list();
        self.expect("do");
        let body = self.parse_list();
        self.expect("done");
        Command::While(Box::new(cond), Box::new(body))
    }

    fn parse_until(&mut self) -> Command {
        self.next();
        let cond = self.parse_list();
        self.expect("do");
        let body = self.parse_list();
        self.expect("done");
        Command::Until(Box::new(cond), Box::new(body))
    }

    fn parse_case(&mut self) -> Command {
        self.next();
        let word = self.expect_word_obj();
        while self.consume_newline() {}
        self.expect("in");
        while self.consume_newline() {}
        let mut arms = Vec::new();
        while !self.at("esac") && !matches!(self.peek(), Token::Eof) {
            self.consume("(");
            let mut pats = Vec::new();
            pats.push(self.expect_word_obj());
            while self.consume("|") {
                pats.push(self.expect_word_obj());
            }
            self.expect(")");
            while self.consume_newline() {}
            let cmd = self.parse_list();
            self.consume(";;");
            self.consume(";&");
            self.consume(";;&");
            while self.consume_newline() {}
            arms.push((pats, cmd));
        }
        self.next();
        Command::Case(word, arms)
    }

    fn parse_function(&mut self, name: String) -> Command {
        self.next();
        self.next();
        self.next();
        while self.consume_newline() {}
        let body = self.parse_command();
        Command::Function(name, Box::new(body))
    }

    fn parse_function_no_paren(&mut self, name: String) -> Command {
        self.next();
        let body = self.parse_command();
        Command::Function(name, Box::new(body))
    }

    fn consume_newline(&mut self) -> bool {
        if matches!(self.peek(), Token::Newline) {
            self.next();
            true
        } else {
            false
        }
    }

    fn parse_simple(&mut self) -> Command {
        let mut assignments = Vec::new();
        let mut words = Vec::new();
        let mut redirs = Vec::new();

        loop {
            match self.peek() {
                Token::Word(w) => {
                    let w = w.clone();
                    self.next();

                    if words.is_empty() {
                        if let Some(assign) = assignment_word(&w) {
                            assignments.push(assign);
                            continue;
                        }
                    }

                    words.push(w);
                }
                Token::IoNumber(n) => {
                    let n = *n;
                    self.next();
                    if let Some(r) = self.parse_redir(Some(n)) {
                        redirs.push(r);
                    }
                }
                Token::Op(ref s) if is_redir_op(s) => {
                    if let Some(r) = self.parse_redir(None) {
                        redirs.push(r);
                    } else {
                        self.next();
                    }
                }
                Token::HereBody(body) => {
                    let body = body.clone();
                    self.next();

                    for r in redirs.iter_mut().rev() {
                        if let Redirect::Here(_, _, _, _, ref mut b) = r {
                            if b.is_none() {
                                *b = Some(body.clone());
                                break;
                            }
                        }
                    }
                }
                _ => break,
            }
        }

        if assignments.is_empty() && words.is_empty() && redirs.is_empty() {
            Command::Empty
        } else {
            Command::Simple(assignments, words, redirs)
        }
    }

    fn parse_redir(&mut self, fd: Option<u32>) -> Option<Redirect> {
        match self.peek() {
            Token::Op(ref s) if s == "<" => {
                self.next();
                Some(Redirect::In(fd, self.expect_word_obj()))
            }
            Token::Op(ref s) if s == ">" => {
                self.next();
                Some(Redirect::Out(fd, self.expect_word_obj()))
            }
            Token::Op(ref s) if s == ">>" => {
                self.next();
                Some(Redirect::Append(fd, self.expect_word_obj()))
            }
            Token::Op(ref s) if s == "<<" => {
                self.next();
                let w = self.expect_word_obj();
                let quoted = word_is_quoted(&w);
                Some(Redirect::Here(fd, w, false, quoted, None))
            }
            Token::Op(ref s) if s == "<<-" => {
                self.next();
                let w = self.expect_word_obj();
                let quoted = word_is_quoted(&w);
                Some(Redirect::Here(fd, w, true, quoted, None))
            }
            Token::Op(ref s) if s == "<&" => {
                self.next();
                Some(Redirect::DupIn(fd, self.expect_word_obj()))
            }
            Token::Op(ref s) if s == ">&" => {
                self.next();
                Some(Redirect::DupOut(fd, self.expect_word_obj()))
            }
            Token::Op(ref s) if s == "<>" => {
                self.next();
                Some(Redirect::ReadWrite(fd, self.expect_word_obj()))
            }
            _ => None,
        }
    }

    fn expect(&mut self, s: &str) {
        if !self.consume(s) {
            panic!(
                "sfsh: syntax error: expected '{}' but found {:?}",
                s,
                self.peek()
            );
        }
    }

    fn expect_word(&mut self) -> String {
        match self.next() {
            Token::Word(w) => word_str(&w),
            _ => panic!("sfsh: syntax error: expected word"),
        }
    }

    fn expect_word_obj(&mut self) -> Word {
        match self.next() {
            Token::Word(w) => w,
            _ => panic!("sfsh: syntax error: expected word"),
        }
    }
}

fn is_redir_op(s: &str) -> bool {
    matches!(s, "<" | ">" | ">>" | "<<" | "<<-" | "<&" | ">&" | "<>")
}

fn word_str(w: &Word) -> String {
    let mut s = String::new();
    for p in &w.0 {
        match p {
            WordPart::Lit(x) | WordPart::SQuote(x) => s.push_str(x),
            WordPart::Tilde(x) => {
                s.push('~');
                s.push_str(x);
            }
            WordPart::Param(n, _) => {
                s.push('$');
                s.push_str(n);
            }
            WordPart::Cmd(c) => {
                s.push_str("$(");
                s.push_str(c);
                s.push(')');
            }
            WordPart::Arith(c) => {
                s.push_str("$((");
                s.push_str(c);
                s.push_str("))");
            }
            WordPart::DQuote(parts) => {
                s.push('"');
                for p in parts {
                    match p {
                        WordPart::Lit(x) | WordPart::SQuote(x) => s.push_str(x),
                        WordPart::Param(n, _) => {
                            s.push('$');
                            s.push_str(n);
                        }
                        WordPart::Cmd(c) => {
                            s.push_str("$(");
                            s.push_str(c);
                            s.push(')');
                        }
                        WordPart::Arith(c) => {
                            s.push_str("$((");
                            s.push_str(c);
                            s.push_str("))");
                        }
                        WordPart::DQuote(inner) => {
                            let inner_word = Word(inner.clone());
                            s.push_str(&word_str(&inner_word));
                        }
                        WordPart::Tilde(x) => {
                            s.push('~');
                            s.push_str(x);
                        }
                    }
                }
                s.push('"');
            }
        }
    }
    s
}

fn assignment_word(w: &Word) -> Option<(String, Word)> {
    let first = match w.0.first() {
        Some(WordPart::Lit(s)) => s,
        _ => return None,
    };

    let eq = first.find('=')?;
    let name = &first[..eq];

    if !is_valid_var_name(name) {
        return None;
    }

    let rest = &first[eq + 1..];
    let mut parts = Vec::new();

    if !rest.is_empty() {
        parts.push(WordPart::Lit(rest.to_string()));
    }

    parts.extend(w.0.iter().skip(1).cloned());

    Some((name.to_string(), Word(parts)))
}

fn is_valid_var_name(s: &str) -> bool {
    let mut chars = s.chars();

    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }

    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn word_is_quoted(w: &Word) -> bool {
    w.0.iter()
        .any(|p| matches!(p, WordPart::SQuote(_) | WordPart::DQuote(_)))
}
