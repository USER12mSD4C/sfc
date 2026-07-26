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
        matches!(self.peek(), Token::Op(ref x) if x == s)
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
            } else if self.at(")")
                || self.at("}")
                || self.at("fi")
                || self.at("done")
                || self.at("esac")
                || self.at("else")
                || self.at("elif")
                || self.at("then")
                || self.at("do")
                || matches!(self.peek(), Token::Eof)
            {
                None
            } else {
                None
            };
            let is_term = op.is_none();
            cmds.push((p, op));
            if is_term {
                break;
            }
        }
        if cmds.is_empty() {
            Command::Empty
        } else {
            Command::List(cmds)
        }
    }

    fn parse_pipeline(&mut self) -> Command {
        let mut cmds = Vec::new();
        let c = self.parse_command();
        if let Command::Empty = c {
            return Command::Empty;
        }
        cmds.push(c);
        while self.consume("|") {
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
            self.consume(")");
            return Command::Subshell(Box::new(c));
        }
        if self.consume("{") {
            let c = self.parse_list();
            self.consume("}");
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
            }
        }
        self.parse_simple()
    }

    fn parse_if(&mut self) -> Command {
        self.next(); // if
        let cond = self.parse_list();
        self.expect("then");
        let then_ = self.parse_list();
        let else_ = if self.consume("else") {
            Some(Box::new(self.parse_list()))
        } else {
            None
        };
        self.expect("fi");
        Command::If(Box::new(cond), Box::new(then_), else_)
    }

    fn parse_for(&mut self) -> Command {
        self.next();
        let var = self.expect_word();
        let mut words = Vec::new();
        if self.consume("in") {
            while self.at_word() {
                words.push(self.expect_word_obj());
            }
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
        self.expect("in");
        let mut arms = Vec::new();
        while !self.at("esac") {
            if self.consume("(") {} // optional
            let mut pats = Vec::new();
            pats.push(self.expect_word_obj());
            while self.consume("|") {
                pats.push(self.expect_word_obj());
            }
            self.consume(")"); // actually required
            let cmd = self.parse_list();
            self.consume(";;"); // or ;& etc
            arms.push((pats, cmd));
        }
        self.next();
        Command::Case(word, arms)
    }

    fn parse_function(&mut self, name: String) -> Command {
        self.next();
        self.next();
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
        let mut words = Vec::new();
        let mut redirs = Vec::new();
        loop {
            match self.peek() {
                Token::Word(w) => {
                    let w = w.clone();
                    self.next();
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
                    let _s = s.clone();
                    self.next();
                    if let Some(r) = self.parse_redir(None) {
                        redirs.push(r);
                    }
                }
                _ => break,
            }
        }
        if words.is_empty() && redirs.is_empty() {
            Command::Empty
        } else {
            Command::Simple(words, redirs)
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
                Some(Redirect::Here(fd, self.expect_word_obj(), false))
            }
            Token::Op(ref s) if s == "<<-" => {
                self.next();
                Some(Redirect::Here(fd, self.expect_word_obj(), true))
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
            panic!("expected {}", s);
        }
    }

    fn expect_word(&mut self) -> String {
        match self.next() {
            Token::Word(w) => word_str(&w),
            _ => panic!("expected word"),
        }
    }

    fn expect_word_obj(&mut self) -> Word {
        match self.next() {
            Token::Word(w) => w,
            _ => panic!("expected word"),
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
            WordPart::Lit(x) | WordPart::SQuote(x) | WordPart::Tilde(x) => s.push_str(x),
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
                    if let WordPart::Lit(x) = p {
                        s.push_str(x);
                    }
                }
                s.push('"');
            }
        }
    }
    s
}
