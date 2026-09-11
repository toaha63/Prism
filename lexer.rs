use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenType {

    Identifier(String),
    Number(f64),
    String(String),
    Char(char),

    Let,
    If,
    ElseIf,
    Else,
    For,
    While,
    Fn,
    Return,
    Print,
    Break,
    Continue,
    Switch,
    Case,
    Default,
    When,
    Enum,
    Dot,
    At,
    Question,
    As,
    Struct,
    Foreach,
    Will,
    Try,
    Catch,
    Throw,
    Parent,

    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Amp,
    Pipe,
    Bang,
    Caret,
    Tilde,
    And,
    Or,
    Eq,
    EqEq,
    Neq,
    Lt,
    Gt,
    Le,
    Ge,
    Assign,
    Import,
    TripleColon,
    DoubleColon,
    Pvt,
    Lambda,

    Comma,
    Colon,
    Semicolon,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,

    EOF,
    Invalid(String),
}

impl fmt::Display for TokenType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TokenType::Identifier(s) => write!(f, "identifier '{}'", s),
            TokenType::Number(n) => write!(f, "number {}", n),
            TokenType::String(s) => write!(f, "string \"{}\"", s),
            TokenType::Char(c) => write!(f, "char '{}'", c),
            TokenType::Let => write!(f, "'$'"),
            TokenType::If => write!(f, "'if'"),
            TokenType::ElseIf => write!(f, "'elseIf'"),
            TokenType::Else => write!(f, "'else'"),
            TokenType::Switch => write!(f, "'switch'"),
            TokenType::Case => write!(f, "'case'"),
            TokenType::When => write!(f, "'when'"),
            TokenType::Default => write!(f, "'default'"),
            TokenType::Struct => write!(f, "'struct'"),
            TokenType::For => write!(f, "'for'"),
            TokenType::Break => write!(f, "'break'"),
            TokenType::Continue => write!(f, "'continue'"),
            TokenType::Enum => write!(f, "'enum'"),
            TokenType::While => write!(f, "'while'"),
            TokenType::Fn => write!(f, "'fn'"),
            TokenType::Return => write!(f, "'return'"),
            TokenType::Print => write!(f, "'print'"),
            TokenType::Import => write!(f, "'import'"),
            TokenType::Pvt => write!(f, "'pvt'"),
            TokenType::As => write!(f, "'as'"),
            TokenType::Foreach => write!(f, "'foreach'"),
            TokenType::Will => write!(f, "'will'"),
            TokenType::Try => write!(f, "'try'"),
            TokenType::Catch => write!(f, "'catch'"),
            TokenType::Throw => write!(f, "'throw'"),
            TokenType::Parent => write!(f, "'parent'"),
            TokenType::Lambda => write!(f, "'lambda'"),
            TokenType::TripleColon => write!(f, "':::'"),
            TokenType:: DoubleColon => write!(f, "'::'"),
            TokenType::Dot => write!(f, "'.'"),
            TokenType::At => write!(f, "'@'"),
            TokenType::Question => write!(f, "'?'"),
            TokenType::Plus => write!(f, "'+'"),
            TokenType::Minus => write!(f, "'-'"),
            TokenType::Star => write!(f, "'*'"),
            TokenType::Slash => write!(f, "'/'"),
            TokenType::Percent => write!(f, "'%'"),
            TokenType::Amp => write!(f, "'&'"),
            TokenType::Pipe => write!(f, "'|'"),
            TokenType::Bang => write!(f, "'!'"),
            TokenType::Caret => write!(f, "'^'"),
            TokenType::Tilde => write!(f, "'~'"),
            TokenType::And => write!(f, "'&&'"),
            TokenType::Or => write!(f, "'||'"),
            TokenType::Eq => write!(f, "'='"),
            TokenType::EqEq => write!(f, "'=='"),
            TokenType::Neq => write!(f, "'!='"),
            TokenType::Lt => write!(f, "'<'"),
            TokenType::Gt => write!(f, "'>'"),
            TokenType::Le => write!(f, "'<='"),
            TokenType::Ge => write!(f, "'>='"),
            TokenType::Assign => write!(f, "'..='"),
            TokenType::Comma => write!(f, "','"),
            TokenType::Colon => write!(f, "':'"),
            TokenType::Semicolon => write!(f, "';'"),
            TokenType::LParen => write!(f, "'('"),
            TokenType::RParen => write!(f, "')'"),
            TokenType::LBrace => write!(f, "'{{'"),
            TokenType::RBrace => write!(f, "'}}'"),
            TokenType::LBracket => write!(f, "'['"),
            TokenType::RBracket => write!(f, "']'"),
            TokenType::EOF => write!(f, "EOF"),
            TokenType::Invalid(s) => write!(f, "invalid token '{}'", s),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Token {
    pub token_type: TokenType,
    pub line: usize,
    pub column: usize,
}

impl Token {
    pub fn new(token_type: TokenType, line: usize, column: usize) -> Self {
        Token {
            token_type,
            line,
            column,
        }
    }
}

pub struct Lexer
{
    source: Vec<char>,
    position: usize,
    line: usize,
    column: usize,
    keywords: HashMap<String, TokenType>,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        let mut keywords = HashMap::new();
        keywords.insert("if".to_string(), TokenType::If);
        keywords.insert("elseIf".to_string(), TokenType::ElseIf);
        keywords.insert("else".to_string(), TokenType::Else);
        keywords.insert("for".to_string(), TokenType::For);
        keywords.insert("fn".to_string(), TokenType::Fn);
        keywords.insert("return".to_string(), TokenType::Return);
        keywords.insert("print".to_string(), TokenType::Print);
        keywords.insert("true".to_string(), TokenType::Identifier("true".to_string()));
        keywords.insert("false".to_string(), TokenType::Identifier("false".to_string()));
        keywords.insert("nil".to_string(), TokenType::Identifier("nil".to_string()));
        keywords.insert("while".to_string(), TokenType::While);
        keywords.insert("break".to_string(), TokenType::Break);
        keywords.insert("continue".to_string(), TokenType::Continue);
        keywords.insert("switch".to_string(), TokenType::Switch);
        keywords.insert("when".to_string(), TokenType::When);
        keywords.insert("case".to_string(), TokenType::Case);
        keywords.insert("default".to_string(), TokenType::Default);
        keywords.insert("enum".to_string(), TokenType::Enum);
        keywords.insert("struct".to_string(), TokenType::Struct);
        keywords.insert("import".to_string(), TokenType::Import);
        keywords.insert("pvt".to_string(), TokenType::Pvt);
        keywords.insert("as".to_string(), TokenType::As);
        keywords.insert("foreach".to_string(), TokenType::Foreach);
        keywords.insert("will".to_string(), TokenType::Will);
        keywords.insert("try".to_string(), TokenType::Try);
        keywords.insert("catch".to_string(), TokenType::Catch);
        keywords.insert("throw".to_string(), TokenType::Throw);
        keywords.insert("parent".to_string(), TokenType::Parent);
        keywords.insert("lambda".to_string(), TokenType::Lambda);

        Lexer {
            source: source.chars().collect(),
            position: 0,
            line: 1,
            column: 1,
            keywords,
        }
    }

    pub fn next_token(&mut self) -> Token {
        self.skip_whitespace_and_comments();

        if self.position >= self.source.len() {
            return Token::new(TokenType::EOF, self.line, self.column);
        }

        let ch = self.source[self.position];
        let start_line = self.line;
        let start_col = self.column;

        match ch {
            '$' => {
                self.advance();
                Token::new(TokenType::Let, start_line, start_col)
            }
            '.' => {
                self.advance();
                Token::new(TokenType::Dot, start_line, start_col)
            }
            '@' => {
                self.advance();
                Token::new(TokenType::At, start_line, start_col)
            }
            '?' => {
                self.advance();
                Token::new(TokenType::Question, start_line, start_col)
            }
            '+' => {
                self.advance();
                Token::new(TokenType::Plus, start_line, start_col)
            }
            '-' => {
                self.advance();
                Token::new(TokenType::Minus, start_line, start_col)
            }
            '*' => {
                self.advance();
                Token::new(TokenType::Star, start_line, start_col)
            }
            '/' => {
                self.advance();
                Token::new(TokenType::Slash, start_line, start_col)
            }
            '%' => {
                self.advance();
                Token::new(TokenType::Percent, start_line, start_col)
            }
            '&' => {
                if self.peek() == '&' {
                    self.advance();
                    self.advance();
                    Token::new(TokenType::And, start_line, start_col)
                } else {
                    self.advance();
                    Token::new(TokenType::Amp, start_line, start_col)
                }
            }
            '|' => {
                if self.peek() == '|' {
                    self.advance();
                    self.advance();
                    Token::new(TokenType::Or, start_line, start_col)
                } else {
                    self.advance();
                    Token::new(TokenType::Pipe, start_line, start_col)
                }
            }
            '!' => {
                if self.peek() == '=' {
                    self.advance();
                    self.advance();
                    Token::new(TokenType::Neq, start_line, start_col)
                } else {
                    self.advance();
                    Token::new(TokenType::Bang, start_line, start_col)
                }
            }
            '^' => {
                self.advance();
                Token::new(TokenType::Caret, start_line, start_col)
            }
            '~' => {
                self.advance();
                Token::new(TokenType::Tilde, start_line, start_col)
            }
            '=' => {
                if self.peek() == '=' {
                    self.advance();
                    self.advance();
                    Token::new(TokenType::EqEq, start_line, start_col)
                } else if self.peek() == '.' && self.peek_next() == '.' {
                    self.advance();
                    self.advance();
                    self.advance();
                    Token::new(TokenType::Assign, start_line, start_col)
                } else {
                    self.advance();
                    Token::new(TokenType::Eq, start_line, start_col)
                }
            }
            '<' => {
                if self.peek() == '=' {
                    self.advance();
                    self.advance();
                    Token::new(TokenType::Le, start_line, start_col)
                } else {
                    self.advance();
                    Token::new(TokenType::Lt, start_line, start_col)
                }
            }
            '>' => {
                if self.peek() == '=' {
                    self.advance();
                    self.advance();
                    Token::new(TokenType::Ge, start_line, start_col)
                } else {
                    self.advance();
                    Token::new(TokenType::Gt, start_line, start_col)
                }
            }
            ',' => {
                self.advance();
                Token::new(TokenType::Comma, start_line, start_col)
            }
            ':' => {
                if self.peek() == ':' && self.peek_next() == ':' {
                    self.advance();
                    self.advance();
                    self.advance();
                    Token::new(TokenType::TripleColon, start_line, start_col)
                } else if self.peek() == ':' {
                    self.advance();
                    self.advance();
                    Token::new(TokenType::DoubleColon, start_line, start_col)
                } else {
                    self.advance();
                    Token::new(TokenType::Colon, start_line, start_col)
                }
            }
            ';' => {
                self.advance();
                Token::new(TokenType::Semicolon, start_line, start_col)
            }
            '(' => {
                self.advance();
                Token::new(TokenType::LParen, start_line, start_col)
            }
            ')' => {
                self.advance();
                Token::new(TokenType::RParen, start_line, start_col)
            }
            '{' => {
                self.advance();
                Token::new(TokenType::LBrace, start_line, start_col)
            }
            '}' => {
                self.advance();
                Token::new(TokenType::RBrace, start_line, start_col)
            }
            '[' => {
                self.advance();
                Token::new(TokenType::LBracket, start_line, start_col)
            }
            ']' => {
                self.advance();
                Token::new(TokenType::RBracket, start_line, start_col)
            }
            '\'' => self.read_char_literal(),
            '"' => self.read_string(),
            _ => {
                if ch.is_alphabetic() || ch == '_' {
                    self.read_identifier()
                } else if ch.is_numeric() {
                    self.read_number()
                } else {
                    let token = Token::new(TokenType::Invalid(ch.to_string()), start_line, start_col);
                    self.advance();
                    token
                }
            }
        }
    }

    fn advance(&mut self) {
        if self.position < self.source.len() {
            if self.source[self.position] == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
            self.position += 1;
        }
    }

    fn peek(&self) -> char {
        if self.position + 1 < self.source.len() {
            self.source[self.position + 1]
        } else {
            '\0'
        }
    }

    fn peek_next(&self) -> char {
        if self.position + 2 < self.source.len() {
            self.source[self.position + 2]
        } else {
            '\0'
        }
    }

    fn skip_whitespace_and_comments(&mut self) {
        while self.position < self.source.len() {
            let ch = self.source[self.position];
            if ch.is_whitespace() {
                self.advance();
            } else if ch == '/' && self.peek() == '/' {
                while self.position < self.source.len() && self.source[self.position] != '\n' {
                    self.advance();
                }
            } else if ch == '/' && self.peek() == '*' {
                self.advance();
                self.advance();
                while self.position + 1 < self.source.len() {
                    if self.source[self.position] == '*' && self.peek() == '/' {
                        self.advance();
                        self.advance();
                        break;
                    }
                    self.advance();
                }
            } else {
                break;
            }
        }
    }

    fn read_identifier(&mut self) -> Token {
        let start = self.position;
        let start_line = self.line;
        let start_col = self.column;

        while self.position < self.source.len() &&
              (self.source[self.position].is_alphanumeric() || self.source[self.position] == '_') {
            self.advance();
        }

        let ident: String = self.source[start..self.position].iter().collect();
        let token_type = self.keywords.get(&ident).cloned().unwrap_or(TokenType::Identifier(ident));
        Token::new(token_type, start_line, start_col)
    }

    fn read_number(&mut self) -> Token {
        let start = self.position;
        let start_line = self.line;
        let start_col = self.column;
        let mut has_dot = false;

        while self.position < self.source.len() {
            let ch = self.source[self.position];
            if ch.is_numeric() {
                self.advance();
            } else if ch == '.' && !has_dot {
                has_dot = true;
                self.advance();
            } else {
                break;
            }
        }

        let num_str: String = self.source[start..self.position].iter().collect();
        let num: f64 = num_str.parse().unwrap();
        Token::new(TokenType::Number(num), start_line, start_col)
    }

    fn read_string(&mut self) -> Token {
        self.advance();
        let _start = self.position;
        let start_line = self.line;
        let start_col = self.column;
        let mut result = String::new();

        while self.position < self.source.len() && self.source[self.position] != '"' {
            let ch = self.source[self.position];
            if ch == '\\' && self.position + 1 < self.source.len() {
                self.advance();
                match self.source[self.position] {
                    'n' => result.push('\n'),
                    't' => result.push('\t'),
                    'r' => result.push('\r'),
                    '\\' => result.push('\\'),
                    '"' => result.push('"'),
                    '\'' => result.push('\''),
                    _ => result.push(self.source[self.position]),
                }
                self.advance();
            } else {
                result.push(ch);
                self.advance();
            }
        }

        self.advance();
        Token::new(TokenType::String(result), start_line, start_col)
    }

    fn read_char_literal(&mut self) -> Token {
        self.advance();
        let start_line = self.line;
        let start_col = self.column;

        let ch = if self.position < self.source.len() {
            let c = self.source[self.position];
            self.advance();

            if c == '\\' && self.position < self.source.len() {
                match self.source[self.position] {
                    'n' => {
                        self.advance();
                        '\n'
                    }
                    't' => {
                        self.advance();
                        '\t'
                    }
                    'r' => {
                        self.advance();
                        '\r'
                    }
                    '\\' => {
                        self.advance();
                        '\\'
                    }
                    '\'' => {
                        self.advance();
                        '\''
                    }
                    '"' => {
                        self.advance();
                        '"'
                    }
                    _ => {
                        self.advance();
                        c
                    }
                }
            } else {
                c
            }
        } else {
            '\0'
        };

        if self.position < self.source.len() && self.source[self.position] == '\'' {
            self.advance();
            Token::new(TokenType::Char(ch), start_line, start_col)
        } else {
            Token::new(TokenType::Invalid("Unterminated character literal".to_string()), start_line, start_col)
        }
    }
}