use crate::lexer::{Lexer, Token, TokenType};

#[derive(Debug, Clone)]
pub enum Expr {
    Literal(Literal),
    Variable(String),
    Binary(Box<Expr>, BinaryOp, Box<Expr>),
    Unary(UnaryOp, Box<Expr>),
    Array(Vec<Expr>),
    ArrayIndex(Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
    Assignment(String, Box<Expr>, bool),
    ArrayAssignment(Box<Expr>, Box<Expr>, Box<Expr>),
    GlobalAssignment(String, Box<Expr>),
    ParentAssignment(String, Box<Expr>),
    EnumAccess(String, String),
    HashMap(Vec<(Expr, Expr)>),
    Struct(String, Vec<(String, Expr)>),
    StructInstance(String, String),
    StructAccess(Box<Expr>, String),
    StructAssignment(Box<Expr>, String, Box<Expr>),
    ModuleCall(String, String, Vec<Expr>),
    ModuleVariable(String, String),
    ModuleEnumAccess(String, String, String),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    ParentAccess(String),
    Lambda(Vec<String>, Vec<Statement>),
}

#[derive(Debug, Clone)]
pub enum Literal {
    Number(f64),
    String(String),
    Char(char),
    Boolean(bool),
    Nil,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    BitAnd,
    BitOr,
    BitXor,
    And,
    Or,
    Eq,
    Neq,
    Lt,
    Gt,
    Le,
    Ge,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Neg,
    Not,
    BitNot,
}

#[derive(Debug, Clone)]
pub enum Statement {
    Expr(Expr),
    Let(String, Expr),
    MultipleLet(Vec<String>, Vec<Expr>),
    Assign(String, Expr),
    If(Expr, Vec<Statement>, Vec<(Expr, Vec<Statement>)>, Option<Vec<Statement>>),
    For(String, Expr, Expr, Expr, Vec<Statement>),
    While(Expr, Vec<Statement>),
    Function(String, Vec<String>, Vec<Statement>, bool),
    Foreach(Expr, Vec<String>, Vec<Statement>),
    Return(Option<Expr>),
    #[allow(dead_code)]
    Block(Vec<Statement>),
    Break,
    Continue,
    Switch(Expr, Vec<SwitchCase>, Option<Vec<Statement>>),
    When(WhenCase),
    Enum(String, Vec<String>),
    Import(String, Option<String>),
    PvtLet(String, Expr),
    PvtFunction(String, Vec<String>, Vec<Statement>),
    Try(Vec<Statement>, String, Vec<Statement>),
    Throw(Expr),
}

#[derive(Debug, Clone)]
pub struct SwitchCase {
    pub value: Expr,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone)]
pub struct WhenCase {
    pub cases: Vec<WhenBranch>,
    pub default_case: Option<Vec<Statement>>,
}

#[derive(Debug, Clone)]
pub struct WhenBranch {
    pub condition: Expr,
    pub body: Vec<Statement>,
}

pub struct Parser {
    lexer: Lexer,
    current_token: Token,
    peek_token: Token,
}

impl Parser {
    pub fn new(mut lexer: Lexer) -> Self {
        let current_token = lexer.next_token();
        let peek_token = lexer.next_token();
        Parser {
            lexer,
            current_token,
            peek_token,
        }
    }

    fn advance(&mut self) {
        self.current_token = self.peek_token.clone();
        self.peek_token = self.lexer.next_token();
    }

    fn expect(&mut self, token_type: TokenType) -> Result<(), String> {
        if std::mem::discriminant(&self.current_token.token_type) == std::mem::discriminant(&token_type) {
            self.advance();
            Ok(())
        } else {
            Err(format!("Expected {} at line {}:{}", token_type,
                       self.current_token.line, self.current_token.column))
        }
    }

    pub fn parse_program(&mut self) -> Result<Vec<Statement>, String> {
        let mut statements = Vec::new();
        while !matches!(self.current_token.token_type, TokenType::EOF) {
            statements.push(self.parse_statement()?);
        }
        Ok(statements)
    }

    fn parse_statement(&mut self) -> Result<Statement, String> {
        match &self.current_token.token_type {
            TokenType::Let => {
                self.advance();

                let first_var = if let TokenType::Identifier(name) = &self.current_token.token_type {
                    let name = name.clone();
                    self.advance();
                    name
                } else {
                    return Err(format!("Expected variable name after $ at line {}:{}",
                                       self.current_token.line, self.current_token.column));
                };

                let mut variables = vec![first_var];
                while let TokenType::Comma = self.current_token.token_type {
                    self.advance();

                    if let TokenType::Let = self.current_token.token_type {
                        self.advance();
                    }

                    if let TokenType::Identifier(name) = &self.current_token.token_type {
                        variables.push(name.clone());
                        self.advance();
                    } else {
                        return Err(format!("Expected variable name after ',' at line {}:{}",
                                          self.current_token.line, self.current_token.column));
                    }
                }

                if let TokenType::Dot = self.current_token.token_type {

                    if variables.len() != 1 {
                        return Err(format!("Struct field assignment with multiple variables not supported at line {}:{}",
                                          self.current_token.line, self.current_token.column));
                    }

                    self.advance();

                    if let TokenType::Identifier(field) = &self.current_token.token_type {
                        let field_name = field.clone();
                        self.advance();
                        self.expect(TokenType::Eq)?;
                        let expr = self.parse_expression()?;
                        self.expect(TokenType::Semicolon)?;

                        return Ok(Statement::Expr(Expr::StructAssignment(
                            Box::new(Expr::Variable(variables[0].clone())),
                            field_name,
                            Box::new(expr)
                        )));
                    } else {
                        return Err(format!("Expected field name after '.' at line {}:{}",
                                          self.current_token.line, self.current_token.column));
                    }
                }

                self.expect(TokenType::Eq)?;

                let mut expressions = Vec::new();
                expressions.push(self.parse_expression()?);

                while let TokenType::Comma = self.current_token.token_type {
                    self.advance();
                    expressions.push(self.parse_expression()?);
                }

                self.expect(TokenType::Semicolon)?;

                if variables.len() == 1 && expressions.len() == 1 {
                    return Ok(Statement::Let(variables[0].clone(), expressions[0].clone()));
                }

                if variables.len() != expressions.len() {
                    return Err(format!(
                        "Multiple assignment: {} variables but {} values at line {}:{}",
                        variables.len(), expressions.len(),
                        self.current_token.line, self.current_token.column
                    ));
                }

                Ok(Statement::MultipleLet(variables, expressions))
            }
            TokenType::Struct => {
                self.parse_struct()
            }
            TokenType::Import => {
                self.advance();

                if let TokenType::String(filename) = &self.current_token.token_type {
                    let filename = filename.clone();
                    self.advance();

                    let alias = if let TokenType::As = self.current_token.token_type {
                        self.advance();

                        if let TokenType::Identifier(alias_name) = &self.current_token.token_type {
                            let alias_name = alias_name.clone();
                            self.advance();
                            Some(alias_name)
                        } else {
                            return Err(format!("Expected alias name after 'as' at line {}:{}",
                                              self.current_token.line, self.current_token.column));
                        }
                    } else {
                        None
                    };

                    self.expect(TokenType::Semicolon)?;
                    Ok(Statement::Import(filename, alias))
                } else {
                    Err(format!("Expected string literal after 'import' at line {}:{}",
                               self.current_token.line, self.current_token.column))
                }
            }
            TokenType::Pvt => {
                self.advance();

                match &self.current_token.token_type {
                    TokenType::Let => {

                        self.advance();
                        if let TokenType::Identifier(name) = &self.current_token.token_type {
                            let var_name = name.clone();
                            self.advance();
                            self.expect(TokenType::Eq)?;
                            let expr = self.parse_expression()?;
                            self.expect(TokenType::Semicolon)?;
                            Ok(Statement::PvtLet(var_name, expr))
                        } else {
                            Err("Expected variable name after pvt $".to_string())
                        }
                    }
                    TokenType::Fn => {

                        self.advance();
                        self.parse_private_function()
                    }
                    _ => Err(format!("Expected '$' or 'fn' after 'pvt' at line {}:{}",
                                    self.current_token.line, self.current_token.column))
                }
            }
            TokenType::If => {
                self.parse_if()
            }
            TokenType::ElseIf => {
                Err(format!("Unexpected 'elseIf' without preceding 'if' at line {}:{}",
                           self.current_token.line, self.current_token.column))
            }
            TokenType::Else => {
                Err(format!("Unexpected 'else' without preceding 'if' at line {}:{}",
                           self.current_token.line, self.current_token.column))
            }
            TokenType::Switch => {
                self.parse_switch()
            }
            TokenType::When => {
                self.parse_when()
            }
            TokenType::For => {
                self.parse_for()
            }
            TokenType::Foreach => {
                self.advance();
                self.parse_foreach()
            }
            TokenType::While => {
                self.parse_while()
            }
            TokenType::Break => {
                self.advance();
                self.expect(TokenType::Semicolon)?;
                Ok(Statement::Break)
            }
            TokenType::Continue => {
                self.advance();
                self.expect(TokenType::Semicolon)?;
                Ok(Statement::Continue)
            }
            TokenType::Try => {
                self.advance();
                self.parse_try()
            }
            TokenType::Throw => {
                self.advance();
                self.parse_throw()
            }
            TokenType::Enum => {
                self.parse_enum()
            }
            TokenType::Fn => {
                self.parse_function()
            }
            TokenType::Return => {
                self.advance();
                let expr = if !matches!(self.current_token.token_type, TokenType::Semicolon) {
                    Some(self.parse_expression()?)
                } else {
                    None
                };
                self.expect(TokenType::Semicolon)?;
                Ok(Statement::Return(expr))
            }
            TokenType::Print => {
                self.advance();
                if let TokenType::LParen = self.current_token.token_type {
                    self.advance();
                    let expr = self.parse_expression()?;
                    self.expect(TokenType::RParen)?;
                    self.expect(TokenType::Semicolon)?;
                    Ok(Statement::Expr(Expr::Call("print".to_string(), vec![expr])))
                } else {
                    Err(format!("Expected '(' after print at line {}:{}",
                               self.current_token.line, self.current_token.column))
                }
            }
            TokenType::Identifier(name) => {
                let ident = name.clone();
                self.advance();

                if let TokenType::Dot = self.current_token.token_type {
                    self.advance();

                    if let TokenType::Identifier(field) = &self.current_token.token_type {
                        let field_name = field.clone();
                        self.advance();

                        if let TokenType::Eq = self.current_token.token_type {
                            self.advance();
                            let value = self.parse_expression()?;
                            self.expect(TokenType::Semicolon)?;
                            return Ok(Statement::Expr(Expr::StructAssignment(
                                Box::new(Expr::Variable(ident)),
                                field_name,
                                Box::new(value)
                            )));
                        } else {

                            let expr = Expr::StructAccess(Box::new(Expr::Variable(ident)), field_name);
                            self.expect(TokenType::Semicolon)?;
                            return Ok(Statement::Expr(expr));
                        }
                    } else {
                        return Err(format!("Expected field name after '.' at line {}:{}",
                                          self.current_token.line, self.current_token.column));
                    }
                }

                if matches!(self.current_token.token_type, TokenType::LParen) {
                    let call_expr = self.parse_function_call(ident)?;

                    match &self.current_token.token_type {
                        TokenType::Eq => {
                            self.advance();
                            let value = self.parse_expression()?;
                            self.expect(TokenType::Semicolon)?;

                            match call_expr {
                                Expr::Call(name, args) if name == "global" && args.len() == 1 => {
                                    if let Expr::Variable(var_name) = &args[0] {
                                        Ok(Statement::Expr(Expr::GlobalAssignment(var_name.clone(), Box::new(value))))
                                    } else {
                                        Err("global() expects a variable name".to_string())
                                    }
                                }
                                Expr::Call(name, args) if name == "parent" && args.len() == 1 => {
                                    if let Expr::Variable(var_name) = &args[0] {
                                        Ok(Statement::Expr(Expr::ParentAssignment(var_name.clone(), Box::new(value))))
                                    } else {
                                        Err("parent() expects a variable name".to_string())
                                    }
                                }
                                _ => {
                                    Err("Only global() or parent() function can be on the left side of assignment".to_string())
                                }
                            }
                        }
                        TokenType::Assign => {
                            self.advance();
                            let value = self.parse_expression()?;
                            self.expect(TokenType::Semicolon)?;

                            match call_expr {
                                Expr::Call(name, args) if name == "global" && args.len() == 1 => {
                                    if let Expr::Variable(var_name) = &args[0] {
                                        Ok(Statement::Expr(Expr::GlobalAssignment(var_name.clone(), Box::new(value))))
                                    } else {
                                        Err("global() expects a variable name".to_string())
                                    }
                                }
                                Expr::Call(name, args) if name == "parent" && args.len() == 1 => {
                                    if let Expr::Variable(var_name) = &args[0] {
                                        Ok(Statement::Expr(Expr::ParentAssignment(var_name.clone(), Box::new(value))))
                                    } else {
                                        Err("parent() expects a variable name".to_string())
                                    }
                                }
                                _ => {
                                    Err("Only global() or parent() function can be on the left side of assignment".to_string())
                                }
                            }
                        }
                        _ => {
                            self.expect(TokenType::Semicolon)?;
                            Ok(Statement::Expr(call_expr))
                        }
                    }
                } else {

                    match &self.current_token.token_type {
                        TokenType::Eq => {
                            self.advance();
                            let expr = self.parse_expression()?;
                            self.expect(TokenType::Semicolon)?;
                            Ok(Statement::Assign(ident, expr))
                        }
                        TokenType::Assign => {
                            self.advance();
                            let expr = self.parse_expression()?;
                            self.expect(TokenType::Semicolon)?;
                            Ok(Statement::Assign(ident, expr))
                        }
                        _ => {
                            let expr = Expr::Variable(ident);
                            self.expect(TokenType::Semicolon)?;
                            Ok(Statement::Expr(expr))
                        }
                    }
                }
            }
            _ => {
                let expr = self.parse_expression()?;
                self.expect(TokenType::Semicolon)?;
                Ok(Statement::Expr(expr))
            }
        }
    }

    fn parse_function_call(&mut self, name: String) -> Result<Expr, String> {
        if let TokenType::LParen = self.current_token.token_type {
            self.advance();
            let mut args = Vec::new();

            if !matches!(self.current_token.token_type, TokenType::RParen) {
                loop {
                    args.push(self.parse_expression()?);
                    match &self.current_token.token_type {
                        TokenType::Comma => {
                            self.advance();
                            continue;
                        }
                        TokenType::RParen => break,
                        _ => return Err(format!("Expected ',' or ')' at line {}:{}",
                                              self.current_token.line, self.current_token.column)),
                    }
                }
            }

            self.expect(TokenType::RParen)?;
            Ok(Expr::Call(name, args))
        } else {
            Err(format!("Expected '(' after function name at line {}:{}",
                       self.current_token.line, self.current_token.column))
        }
    }

    fn parse_function(&mut self) -> Result<Statement, String> {
        self.advance();

        let func_name = if let TokenType::Identifier(name) = &self.current_token.token_type {
            let name = name.clone();
            self.advance();
            name
        } else {
            return Err(format!("Expected function name at line {}:{}",
                              self.current_token.line, self.current_token.column));
        };

        self.expect(TokenType::LParen)?;
        let mut params = Vec::new();
        if !matches!(self.current_token.token_type, TokenType::RParen) {
            loop {
                if let TokenType::Let = self.current_token.token_type {
                    self.advance();
                }
                if let TokenType::Identifier(name) = &self.current_token.token_type {
                    params.push(name.clone());
                    self.advance();
                } else {
                    return Err(format!("Expected parameter name at line {}:{}",
                                      self.current_token.line, self.current_token.column));
                }
                match &self.current_token.token_type {
                    TokenType::Comma => {
                        self.advance();
                        continue;
                    }
                    TokenType::RParen => break,
                    _ => return Err(format!("Expected ',' or ')' at line {}:{}",
                                          self.current_token.line, self.current_token.column)),
                }
            }
        }
        self.expect(TokenType::RParen)?;
        self.expect(TokenType::Colon)?;
        self.expect(TokenType::LBrace)?;

        let mut body = Vec::new();
        while !matches!(self.current_token.token_type, TokenType::RBrace) {
            body.push(self.parse_statement()?);
        }
        self.expect(TokenType::RBrace)?;

        Ok(Statement::Function(func_name, params, body, false))
    }

    fn parse_struct(&mut self) -> Result<Statement, String> {
        self.advance();

        let struct_name = if let TokenType::Identifier(name) = &self.current_token.token_type {
            let name = name.clone();
            self.advance();
            name
        } else {
            return Err(format!("Expected struct name at line {}:{}",
                              self.current_token.line, self.current_token.column));
        };

        self.expect(TokenType::Colon)?;

        self.expect(TokenType::LBrace)?;

        let mut fields = Vec::new();

        while !matches!(self.current_token.token_type, TokenType::RBrace) {
            if let TokenType::Let = self.current_token.token_type {
                self.advance();
            }

            if let TokenType::Identifier(name) = &self.current_token.token_type {
                let field_name = name.clone();
                self.advance();

                self.expect(TokenType::Eq)?;

                let field_value = self.parse_expression()?;

                fields.push((field_name, field_value));

                match &self.current_token.token_type {
                    TokenType::Semicolon => {
                        self.advance();
                        continue;
                    }
                    TokenType::RBrace => break,
                    _ => return Err(format!("Expected ';' or '}}' at line {}:{}",
                                          self.current_token.line, self.current_token.column)),
                }
            } else {
                return Err(format!("Expected field name at line {}:{}",
                                  self.current_token.line, self.current_token.column));
            }
        }

        self.expect(TokenType::RBrace)?;

        Ok(Statement::Expr(Expr::Struct(struct_name, fields)))
    }
    fn parse_if(&mut self) -> Result<Statement, String> {
        self.advance();
        let condition = self.parse_expression()?;
        self.expect(TokenType::Colon)?;
        self.expect(TokenType::LBrace)?;

        let mut then_branch = Vec::new();
        while !matches!(self.current_token.token_type, TokenType::RBrace) {
            then_branch.push(self.parse_statement()?);
        }
        self.expect(TokenType::RBrace)?;

        let mut else_if_branches = Vec::new();
        let mut else_branch = None;

        loop {
            match &self.current_token.token_type {
                TokenType::ElseIf => {
                    self.advance();
                    let condition = self.parse_expression()?;
                    self.expect(TokenType::Colon)?;
                    self.expect(TokenType::LBrace)?;

                    let mut body = Vec::new();
                    while !matches!(self.current_token.token_type, TokenType::RBrace) {
                        body.push(self.parse_statement()?);
                    }
                    self.expect(TokenType::RBrace)?;
                    else_if_branches.push((condition, body));
                }
                TokenType::Else => {
                    self.advance();
                    self.expect(TokenType::Colon)?;
                    self.expect(TokenType::LBrace)?;

                    let mut body = Vec::new();
                    while !matches!(self.current_token.token_type, TokenType::RBrace) {
                        body.push(self.parse_statement()?);
                    }
                    self.expect(TokenType::RBrace)?;
                    else_branch = Some(body);
                    break;
                }
                _ => break,
            }
        }

        Ok(Statement::If(condition, then_branch, else_if_branches, else_branch))
    }
    fn parse_switch(&mut self) -> Result<Statement, String> {
        self.advance();

        self.expect(TokenType::LParen)?;
        let condition = self.parse_expression()?;
        self.expect(TokenType::RParen)?;

        self.expect(TokenType::Colon)?;

        self.expect(TokenType::LBrace)?;

        let mut cases = Vec::new();
        let mut default_case = None;

        while !matches!(self.current_token.token_type, TokenType::RBrace) {
            match &self.current_token.token_type {
                TokenType::Case => {
                    self.advance();
                    let case_value = self.parse_expression()?;
                    self.expect(TokenType::Colon)?;
                    self.expect(TokenType::LBrace)?;

                    let mut body = Vec::new();
                    while !matches!(self.current_token.token_type, TokenType::RBrace) {
                        body.push(self.parse_statement()?);
                    }
                    self.expect(TokenType::RBrace)?;

                    cases.push(SwitchCase {
                        value: case_value,
                        body,
                    });
                }
                TokenType::Default => {
                    self.advance();
                    self.expect(TokenType::Colon)?;
                    self.expect(TokenType::LBrace)?;

                    let mut body = Vec::new();
                    while !matches!(self.current_token.token_type, TokenType::RBrace) {
                        body.push(self.parse_statement()?);
                    }
                    self.expect(TokenType::RBrace)?;

                    default_case = Some(body);
                }
                _ => {
                    return Err(format!("Expected 'case' or 'default' at line {}:{}",
                                      self.current_token.line, self.current_token.column));
                }
            }
        }

        self.expect(TokenType::RBrace)?;

        Ok(Statement::Switch(condition, cases, default_case))
    }

    fn parse_when(&mut self) -> Result<Statement, String> {
        self.advance();

        self.expect(TokenType::LParen)?;
        let _value = self.parse_expression()?;
        self.expect(TokenType::RParen)?;

        self.expect(TokenType::Colon)?;

        self.expect(TokenType::LBrace)?;

        let mut cases = Vec::new();
        let mut default_case = None;

        while !matches!(self.current_token.token_type, TokenType::RBrace) {
            match &self.current_token.token_type {
                TokenType::Case => {
                    self.advance();
                    let condition = self.parse_expression()?;
                    self.expect(TokenType::Colon)?;
                    self.expect(TokenType::LBrace)?;

                    let mut body = Vec::new();
                    while !matches!(self.current_token.token_type, TokenType::RBrace) {
                        body.push(self.parse_statement()?);
                    }
                    self.expect(TokenType::RBrace)?;

                    cases.push(WhenBranch {
                        condition,
                        body,
                    });
                }
                TokenType::Default => {
                    self.advance();
                    self.expect(TokenType::Colon)?;
                    self.expect(TokenType::LBrace)?;

                    let mut body = Vec::new();
                    while !matches!(self.current_token.token_type, TokenType::RBrace) {
                        body.push(self.parse_statement()?);
                    }
                    self.expect(TokenType::RBrace)?;

                    default_case = Some(body);
                }
                _ => {
                    return Err(format!("Expected 'case' or 'default' at line {}:{}",
                                      self.current_token.line, self.current_token.column));
                }
            }
        }

        self.expect(TokenType::RBrace)?;

        Ok(Statement::When(WhenCase {
            cases,
            default_case,
        }))
    }

    fn parse_for(&mut self) -> Result<Statement, String> {
        self.advance();
    
        let var_name = match &self.current_token.token_type {
            TokenType::Identifier(name) => {
                let name = name.clone();
                self.advance();
                self.expect(TokenType::Comma)?;
                name
            }
            TokenType::Let => {
                self.advance();
                if let TokenType::Identifier(name) = &self.current_token.token_type {
                    let name = name.clone();
                    self.advance();
                    self.expect(TokenType::Comma)?;
                    name
                } else {
                    return Err(format!("Expected variable name after $ at line {}:{}",
                                      self.current_token.line, self.current_token.column));
                }
            }
            _ => {
                return Err(format!("Expected loop variable name or '_' at line {}:{}",
                                  self.current_token.line, self.current_token.column));
            }
        };
    
        let start = self.parse_expression()?;
        self.expect(TokenType::Identifier("to".to_string()))?;
        let end = self.parse_expression()?;
    
        let step = if let TokenType::Identifier(name) = &self.current_token.token_type {
            if name == "encrease" {
                self.advance();
                self.parse_expression()?
            } else {
                Expr::Literal(Literal::Number(1.0))
            }
        } else {
            Expr::Literal(Literal::Number(1.0))
        };
    
        self.expect(TokenType::Colon)?;
        self.expect(TokenType::LBrace)?;
    
        let mut body = Vec::new();
        while !matches!(self.current_token.token_type, TokenType::RBrace) {
            body.push(self.parse_statement()?);
        }
        self.expect(TokenType::RBrace)?;
    
        Ok(Statement::For(var_name, start, end, step, body))
    }

    fn parse_while(&mut self) -> Result<Statement, String> {
        self.advance();

        let condition = if let TokenType::LParen = self.current_token.token_type {
            self.advance();
            let expr = self.parse_expression()?;
            self.expect(TokenType::RParen)?;
            expr
        } else {
            self.parse_expression()?
        };

        self.expect(TokenType::Colon)?;
        self.expect(TokenType::LBrace)?;

        let mut body = Vec::new();
        while !matches!(self.current_token.token_type, TokenType::RBrace) {
            body.push(self.parse_statement()?);
        }
        self.expect(TokenType::RBrace)?;

        Ok(Statement::While(condition, body))
    }

    fn parse_foreach(&mut self) -> Result<Statement, String> {
        let iterable = self.parse_expression()?;

        self.expect(TokenType::Will)?;

        let mut variables = Vec::new();

        if let TokenType::Let = self.current_token.token_type {
            self.advance();
        }

        if let TokenType::Identifier(name) = &self.current_token.token_type {
            let var_name = name.clone();
            self.advance();
            variables.push(var_name);

            if let TokenType::Comma = self.current_token.token_type {
                self.advance();

                if let TokenType::Let = self.current_token.token_type {
                    self.advance();
                }

                if let TokenType::Identifier(name2) = &self.current_token.token_type {
                    let var_name2 = name2.clone();
                    self.advance();
                    variables.push(var_name2);
                } else {
                    return Err(format!("Expected second variable name after ',' at line {}:{}",
                                      self.current_token.line, self.current_token.column));
                }
            }
        } else {
            return Err(format!("Expected variable name after 'will' at line {}:{}",
                              self.current_token.line, self.current_token.column));
        }

        self.expect(TokenType::Colon)?;
        self.expect(TokenType::LBrace)?;

        let mut body = Vec::new();
        while !matches!(self.current_token.token_type, TokenType::RBrace) {
            body.push(self.parse_statement()?);
        }
        self.expect(TokenType::RBrace)?;

        Ok(Statement::Foreach(iterable, variables, body))
    }

    fn parse_try(&mut self) -> Result<Statement, String> {
        self.expect(TokenType::LBrace)?;
        let mut try_block = Vec::new();
        while !matches!(self.current_token.token_type, TokenType::RBrace) {
            try_block.push(self.parse_statement()?);
        }
        self.expect(TokenType::RBrace)?;

        self.expect(TokenType::Catch)?;

        self.expect(TokenType::LParen)?;

        let error_var = if let TokenType::Let = self.current_token.token_type {
            self.advance();
            if let TokenType::Identifier(name) = &self.current_token.token_type {
                let var_name = name.clone();
                self.advance();
                var_name
            } else {
                return Err(format!("Expected variable name after '$' at line {}:{}",
                                  self.current_token.line, self.current_token.column));
            }
        } else {
            return Err(format!("Expected '$' at line {}:{}",
                              self.current_token.line, self.current_token.column));
        };

        self.expect(TokenType::RParen)?;

        self.expect(TokenType::LBrace)?;
        let mut catch_block = Vec::new();
        while !matches!(self.current_token.token_type, TokenType::RBrace) {
            catch_block.push(self.parse_statement()?);
        }
        self.expect(TokenType::RBrace)?;

        Ok(Statement::Try(try_block, error_var, catch_block))
    }

    fn parse_throw(&mut self) -> Result<Statement, String> {
        let expr = self.parse_expression()?;
        self.expect(TokenType::Semicolon)?;
        Ok(Statement::Throw(expr))
    }

    fn parse_expression(&mut self) -> Result<Expr, String> {
        self.parse_ternary()
    }

    fn parse_ternary(&mut self) -> Result<Expr, String> {
        let condition = self.parse_assignment()?;

        if let TokenType::Question = self.current_token.token_type {
            self.advance();

            let true_expr = self.parse_expression()?;

            self.expect(TokenType::Colon)?;

            let false_expr = self.parse_expression()?;

            return Ok(Expr::Ternary(Box::new(condition), Box::new(true_expr), Box::new(false_expr)));
        }
        Ok(condition)
    }

    fn parse_assignment(&mut self) -> Result<Expr, String> {
        let expr = self.parse_logical_or()?;

        match &self.current_token.token_type {
            TokenType::Eq => {
                self.advance();
                let value = self.parse_expression()?;
                match expr {
                    Expr::Variable(name) => Ok(Expr::Assignment(name, Box::new(value), false)),
                    Expr::ParentAccess(name) => Ok(Expr::ParentAssignment(name, Box::new(value))),
                    Expr::StructAccess(obj, field) => {
                        Ok(Expr::StructAssignment(obj, field, Box::new(value)))
                    }
                    Expr::Call(name, args) if name == "global" && args.len() == 1 => {
                        if let Expr::Variable(var_name) = &args[0] {
                            Ok(Expr::GlobalAssignment(var_name.clone(), Box::new(value)))
                        } else {
                            Err("global() expects a variable name".to_string())
                        }
                    }
                    Expr::Call(name, args) if name == "parent" && args.len() == 1 => {
                        if let Expr::Variable(var_name) = &args[0] {
                            Ok(Expr::ParentAssignment(var_name.clone(), Box::new(value)))
                        } else {
                            Err("parent() expects a variable name".to_string())
                        }
                    }
                    Expr::ArrayIndex(arr, idx) => Ok(Expr::ArrayAssignment(arr, idx, Box::new(value))),
                    _ => Err("Invalid assignment target".to_string()),
                }
            }
            TokenType::Assign => {
                self.advance();
                let value = self.parse_expression()?;
                match expr {
                    Expr::Variable(name) => Ok(Expr::Assignment(name, Box::new(value), true)),
                    Expr::ParentAccess(name) => Ok(Expr::ParentAssignment(name, Box::new(value))),
                    Expr::StructAccess(obj, field) => {
                        Ok(Expr::StructAssignment(obj, field, Box::new(value)))
                    }
                    Expr::Call(name, args) if name == "global" && args.len() == 1 => {
                        if let Expr::Variable(var_name) = &args[0] {
                            Ok(Expr::GlobalAssignment(var_name.clone(), Box::new(value)))
                        } else {
                            Err("global() expects a variable name".to_string())
                        }
                    }
                    Expr::Call(name, args) if name == "parent" && args.len() == 1 => {
                        if let Expr::Variable(var_name) = &args[0] {
                            Ok(Expr::ParentAssignment(var_name.clone(), Box::new(value)))
                        } else {
                            Err("parent() expects a variable name".to_string())
                        }
                    }
                    _ => Err("Invalid reassignment target".to_string()),
                }
            }
            _ => Ok(expr),
        }
    }

    fn parse_enum(&mut self) -> Result<Statement, String> {
        self.advance();

        let enum_name = if let TokenType::Identifier(name) = &self.current_token.token_type {
            let name = name.clone();
            self.advance();
            name
        } else {
            return Err(format!("Expected enum name at line {}:{}",
                              self.current_token.line, self.current_token.column));
        };

        self.expect(TokenType::Colon)?;

        self.expect(TokenType::LBrace)?;

        let mut variants = Vec::new();

        while !matches!(self.current_token.token_type, TokenType::RBrace) {
            if let TokenType::Identifier(name) = &self.current_token.token_type {
                variants.push(name.clone());
                self.advance();

                if matches!(self.current_token.token_type, TokenType::Comma) {
                    self.advance();
                }
            } else {
                return Err(format!("Expected enum variant at line {}:{}",
                                  self.current_token.line, self.current_token.column));
            }
        }

        self.expect(TokenType::RBrace)?;

        Ok(Statement::Enum(enum_name, variants))
    }

    fn parse_logical_or(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_logical_and()?;

        while let TokenType::Or = self.current_token.token_type {
            self.advance();
            let right = self.parse_logical_and()?;
            expr = Expr::Binary(Box::new(expr), BinaryOp::Or, Box::new(right));
        }

        Ok(expr)
    }

    fn parse_logical_and(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_equality()?;

        while let TokenType::And = self.current_token.token_type {
            self.advance();
            let right = self.parse_equality()?;
            expr = Expr::Binary(Box::new(expr), BinaryOp::And, Box::new(right));
        }

        Ok(expr)
    }

    fn parse_equality(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_comparison()?;

        loop {
            match &self.current_token.token_type {
                TokenType::EqEq => {
                    self.advance();
                    let right = self.parse_comparison()?;
                    expr = Expr::Binary(Box::new(expr), BinaryOp::Eq, Box::new(right));
                }
                TokenType::Neq => {
                    self.advance();
                    let right = self.parse_comparison()?;
                    expr = Expr::Binary(Box::new(expr), BinaryOp::Neq, Box::new(right));
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    fn parse_private_function(&mut self) -> Result<Statement, String> {
        let func_name = if let TokenType::Identifier(name) = &self.current_token.token_type {
            let name = name.clone();
            self.advance();
            name
        } else {
            return Err(format!("Expected function name at line {}:{}",
                              self.current_token.line, self.current_token.column));
        };

        self.expect(TokenType::LParen)?;
        let mut params = Vec::new();
        if !matches!(self.current_token.token_type, TokenType::RParen) {
            loop {
                if let TokenType::Let = self.current_token.token_type {
                    self.advance();
                }
                if let TokenType::Identifier(name) = &self.current_token.token_type {
                    params.push(name.clone());
                    self.advance();
                } else {
                    return Err(format!("Expected parameter name at line {}:{}",
                                      self.current_token.line, self.current_token.column));
                }
                match &self.current_token.token_type {
                    TokenType::Comma => {
                        self.advance();
                        continue;
                    }
                    TokenType::RParen => break,
                    _ => return Err(format!("Expected ',' or ')' at line {}:{}",
                                          self.current_token.line, self.current_token.column)),
                }
            }
        }
        self.expect(TokenType::RParen)?;
        self.expect(TokenType::Colon)?;
        self.expect(TokenType::LBrace)?;

        let mut body = Vec::new();
        while !matches!(self.current_token.token_type, TokenType::RBrace) {
            body.push(self.parse_statement()?);
        }
        self.expect(TokenType::RBrace)?;

        Ok(Statement::PvtFunction(func_name, params, body))
    }
    fn parse_comparison(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_term()?;

        loop {
            match &self.current_token.token_type {
                TokenType::Lt => {
                    self.advance();
                    let right = self.parse_term()?;
                    expr = Expr::Binary(Box::new(expr), BinaryOp::Lt, Box::new(right));
                }
                TokenType::Gt => {
                    self.advance();
                    let right = self.parse_term()?;
                    expr = Expr::Binary(Box::new(expr), BinaryOp::Gt, Box::new(right));
                }
                TokenType::Le => {
                    self.advance();
                    let right = self.parse_term()?;
                    expr = Expr::Binary(Box::new(expr), BinaryOp::Le, Box::new(right));
                }
                TokenType::Ge => {
                    self.advance();
                    let right = self.parse_term()?;
                    expr = Expr::Binary(Box::new(expr), BinaryOp::Ge, Box::new(right));
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    fn parse_term(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_factor()?;

        loop {
            match &self.current_token.token_type {
                TokenType::Plus => {
                    self.advance();
                    let right = self.parse_factor()?;
                    expr = Expr::Binary(Box::new(expr), BinaryOp::Add, Box::new(right));
                }
                TokenType::Minus => {
                    self.advance();
                    let right = self.parse_factor()?;
                    expr = Expr::Binary(Box::new(expr), BinaryOp::Sub, Box::new(right));
                }
                TokenType::Pipe => {
                    self.advance();
                    let right = self.parse_factor()?;
                    expr = Expr::Binary(Box::new(expr), BinaryOp::BitOr, Box::new(right));
                }
                TokenType::Caret => {
                    self.advance();
                    let right = self.parse_factor()?;
                    expr = Expr::Binary(Box::new(expr), BinaryOp::BitXor, Box::new(right));
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    fn parse_factor(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_unary()?;

        loop {
            match &self.current_token.token_type {
                TokenType::Star => {
                    self.advance();
                    let right = self.parse_unary()?;
                    expr = Expr::Binary(Box::new(expr), BinaryOp::Mul, Box::new(right));
                }
                TokenType::Slash => {
                    self.advance();
                    let right = self.parse_unary()?;
                    expr = Expr::Binary(Box::new(expr), BinaryOp::Div, Box::new(right));
                }
                TokenType::Percent => {
                    self.advance();
                    let right = self.parse_unary()?;
                    expr = Expr::Binary(Box::new(expr), BinaryOp::Mod, Box::new(right));
                }
                TokenType::Amp => {
                    self.advance();
                    let right = self.parse_unary()?;
                    expr = Expr::Binary(Box::new(expr), BinaryOp::BitAnd, Box::new(right));
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        match &self.current_token.token_type {
            TokenType::Minus => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::Unary(UnaryOp::Neg, Box::new(expr)))
            }
            TokenType::Bang => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::Unary(UnaryOp::Not, Box::new(expr)))
            }
            TokenType::Tilde => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::Unary(UnaryOp::BitNot, Box::new(expr)))
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        match &self.current_token.token_type {
            TokenType::Number(n) => {
                let num = *n;
                self.advance();
                Ok(Expr::Literal(Literal::Number(num)))
            }
            TokenType::String(s) => {
                let str_val = s.clone();
                self.advance();
                Ok(Expr::Literal(Literal::String(str_val)))
            }
            TokenType::Char(c) => {
                let ch = *c;
                self.advance();
                Ok(Expr::Literal(Literal::Char(ch)))
            }
            TokenType::Let => {
                self.advance();
                if let TokenType::Identifier(name) = &self.current_token.token_type {
                    let var_name = name.clone();
                    self.advance();

                    if let TokenType::Dot = self.current_token.token_type {
                        self.advance();

                        if let TokenType::Identifier(field) = &self.current_token.token_type {
                            let field_name = field.clone();
                            self.advance();

                            if let TokenType::Eq = self.current_token.token_type {
                                self.advance();
                                let value = self.parse_expression()?;
                                return Ok(Expr::StructAssignment(
                                    Box::new(Expr::Variable(var_name)),
                                    field_name,
                                    Box::new(value)
                                ));
                            } else {
                                return Ok(Expr::StructAccess(
                                    Box::new(Expr::Variable(var_name)),
                                    field_name
                                ));
                            }
                        } else {
                            return Err(format!("Expected field name after '.' at line {}:{}",
                                              self.current_token.line, self.current_token.column));
                        }
                    }

                    if let TokenType::LParen = self.current_token.token_type {
                        self.advance();
                        let mut args = Vec::new();
                        if !matches!(self.current_token.token_type, TokenType::RParen) {
                            loop {
                                args.push(self.parse_expression()?);
                                match &self.current_token.token_type {
                                    TokenType::Comma => {
                                        self.advance();
                                        continue;
                                    }
                                    TokenType::RParen => break,
                                    _ => return Err(format!("Expected ',' or ')' at line {}:{}",
                                                          self.current_token.line, self.current_token.column)),
                                }
                            }
                        }
                        self.expect(TokenType::RParen)?;
                        return Ok(Expr::Call(var_name, args));
                    }

                    if let TokenType::LBracket = self.current_token.token_type {
                        self.advance();
                        let index = self.parse_expression()?;
                        self.expect(TokenType::RBracket)?;
                        return Ok(Expr::ArrayIndex(Box::new(Expr::Variable(var_name)), Box::new(index)));
                    }

                    Ok(Expr::Variable(var_name))
                } else {
                    Err(format!("Expected variable name after $ at line {}:{}",
                               self.current_token.line, self.current_token.column))
                }
            }
            TokenType::Identifier(name) if name == "true" => {
                self.advance();
                Ok(Expr::Literal(Literal::Boolean(true)))
            }
            TokenType::Identifier(name) if name == "false" => {
                self.advance();
                Ok(Expr::Literal(Literal::Boolean(false)))
            }
            TokenType::Identifier(name) if name == "nil" => {
                self.advance();
                Ok(Expr::Literal(Literal::Nil))
            }
            TokenType::Lambda => {
                self.advance();
                self.expect(TokenType::Fn)?;

                let params = if let TokenType::LParen = self.current_token.token_type {
                    self.advance();
                    let mut params = Vec::new();
                    if !matches!(self.current_token.token_type, TokenType::RParen) {
                        loop {
                            if let TokenType::Let = self.current_token.token_type {
                                self.advance();
                            }
                            if let TokenType::Identifier(name) = &self.current_token.token_type {
                                params.push(name.clone());
                                self.advance();
                            } else {
                                return Err(format!("Expected parameter name at line {}:{}",
                                                  self.current_token.line, self.current_token.column));
                            }
                            match &self.current_token.token_type {
                                TokenType::Comma => {
                                    self.advance();
                                    continue;
                                }
                                TokenType::RParen => break,
                                _ => return Err(format!("Expected ',' or ')' at line {}:{}",
                                                      self.current_token.line, self.current_token.column)),
                            }
                        }
                    }
                    self.expect(TokenType::RParen)?;
                    params
                } else {
                    let mut params = Vec::new();
                    if let TokenType::Let = self.current_token.token_type {
                        self.advance();
                    }
                    if let TokenType::Identifier(name) = &self.current_token.token_type {
                        params.push(name.clone());
                        self.advance();
                    }
                    params
                };

                self.expect(TokenType::Colon)?;

                if self.current_token.token_type != TokenType::LBrace {

                    if let TokenType::Return = self.current_token.token_type {

                        self.advance();
                        let expr = self.parse_expression()?;
                        let mut body = Vec::new();
                        body.push(Statement::Return(Some(expr)));
                        return Ok(Expr::Lambda(params, body));
                    } else {

                        let expr = self.parse_expression()?;
                        let mut body = Vec::new();

                        body.push(Statement::Return(Some(expr)));
                        return Ok(Expr::Lambda(params, body));
                    }
                }

                self.expect(TokenType::LBrace)?;

                let mut body = Vec::new();
                while !matches!(self.current_token.token_type, TokenType::RBrace) {
                    body.push(self.parse_statement()?);
                }
                self.expect(TokenType::RBrace)?;

                Ok(Expr::Lambda(params, body))
            }
           TokenType::Parent => {
                self.advance();

                if let TokenType::LParen = self.current_token.token_type {

                    self.advance();

                    if let TokenType::Let = self.current_token.token_type {
                        self.advance();
                    }

                    if let TokenType::Identifier(name) = &self.current_token.token_type {
                        let var_name = name.clone();
                        self.advance();
                        self.expect(TokenType::RParen)?;
                        Ok(Expr::ParentAccess(var_name))
                    } else {
                        Err(format!("Expected variable name after parent( at line {}:{}",
                                   self.current_token.line, self.current_token.column))
                    }
                } else {

                    if let TokenType::Let = self.current_token.token_type {
                        self.advance();
                    }

                    if let TokenType::Identifier(name) = &self.current_token.token_type {
                        let var_name = name.clone();
                        self.advance();
                        Ok(Expr::ParentAccess(var_name))
                    } else {
                        Err(format!("Expected variable name after parent at line {}:{}",
                                   self.current_token.line, self.current_token.column))
                    }
                }
            }
            TokenType::At => {
                self.advance();

                if let TokenType::Identifier(name) = &self.current_token.token_type {
                    let struct_name = name.clone();
                    self.advance();
                    Ok(Expr::StructInstance(struct_name.clone(), struct_name))
                } else {
                    Err(format!("Expected struct name after '@' at line {}:{}",
                              self.current_token.line, self.current_token.column))
                }
            }
            TokenType::LBrace => {
                self.advance();
                let mut pairs = Vec::new();

                if !matches!(self.current_token.token_type, TokenType::RBrace) {
                    loop {

                        let key_expr = self.parse_expression()?;

                        self.expect(TokenType::Colon)?;

                        let value_expr = self.parse_expression()?;

                        pairs.push((key_expr, value_expr));

                        match &self.current_token.token_type {
                            TokenType::Comma => {
                                self.advance();
                                continue;
                            }
                            TokenType::RBrace => break,
                            _ => return Err(format!("Expected ',' or '}}' at line {}:{}",
                                                  self.current_token.line, self.current_token.column)),
                        }
                    }
                }

                self.expect(TokenType::RBrace)?;
                Ok(Expr::HashMap(pairs))
            }
            TokenType::Identifier(name) => {
                let ident = name.clone();
                self.advance();

                if let TokenType::TripleColon = self.current_token.token_type {
                    self.advance();

                    if let TokenType::Identifier(name) = &self.current_token.token_type {
                        let name = name.clone();
                        self.advance();

                        if let TokenType::DoubleColon = self.current_token.token_type {
                            self.advance();

                            if let TokenType::Identifier(variant) = &self.current_token.token_type {
                                let variant_name = variant.clone();
                                self.advance();

                                if let TokenType::LParen = self.current_token.token_type {
                                    return Err(format!(
                                        "Enum variant '{}::{}' cannot be called as a function at line {}:{}",
                                        name, variant_name,
                                        self.current_token.line, self.current_token.column
                                    ));
                                }

                                return Ok(Expr::ModuleEnumAccess(ident, name, variant_name));
                            } else {
                                return Err(format!(
                                    "Expected enum variant name after '::' at line {}:{}",
                                    self.current_token.line, self.current_token.column
                                ));
                            }
                        }

                        if let TokenType::LParen = self.current_token.token_type {

                            self.advance();
                            let mut args = Vec::new();
                            if !matches!(self.current_token.token_type, TokenType::RParen) {
                                loop {
                                    args.push(self.parse_expression()?);
                                    match &self.current_token.token_type {
                                        TokenType::Comma => {
                                            self.advance();
                                            continue;
                                        }
                                        TokenType::RParen => break,
                                        _ => return Err(format!("Expected ',' or ')' at line {}:{}",
                                                              self.current_token.line, self.current_token.column)),
                                    }
                                }
                            }
                            self.expect(TokenType::RParen)?;
                            return Ok(Expr::ModuleCall(ident, name, args));
                        } else {

                            if let TokenType::Eq = self.current_token.token_type {
                                return Err(format!(
                                    "Cannot assign to module variable '{}' from module '{}'. \
                                     Module variables are read-only.",
                                    name, ident
                                ));
                            }
                            return Ok(Expr::ModuleVariable(ident, name));
                        }
                    } else {
                        return Err(format!(
                            "Expected name after ':::' at line {}:{}",
                            self.current_token.line, self.current_token.column
                        ));
                    }
                }

                if let TokenType::DoubleColon = self.current_token.token_type {
                    self.advance();

                    if let TokenType::Identifier(variant) = &self.current_token.token_type {
                        let variant_name = variant.clone();
                        self.advance();
                        return Ok(Expr::EnumAccess(ident, variant_name));
                    } else {
                        return Err(format!("Expected enum variant after '::' at line {}:{}",
                                          self.current_token.line, self.current_token.column));
                    }
                }

                if let TokenType::Dot = self.current_token.token_type {
                    self.advance();

                    if let TokenType::Identifier(field) = &self.current_token.token_type {
                        let field_name = field.clone();
                        self.advance();

                        if let TokenType::Eq = self.current_token.token_type {
                            self.advance();
                            let value = self.parse_expression()?;
                            return Ok(Expr::StructAssignment(
                                Box::new(Expr::Variable(ident)),
                                field_name,
                                Box::new(value)
                            ));
                        } else {
                            return Ok(Expr::StructAccess(
                                Box::new(Expr::Variable(ident)),
                                field_name
                            ));
                        }
                    } else {
                        return Err(format!("Expected field name after '.' at line {}:{}",
                                          self.current_token.line, self.current_token.column));
                    }
                }

                if let TokenType::Colon = self.current_token.token_type {
                    self.advance();
                    if let TokenType::Colon = self.current_token.token_type {
                        self.advance();

                        if let TokenType::Identifier(variant) = &self.current_token.token_type {
                            let variant_name = variant.clone();
                            self.advance();
                            return Ok(Expr::EnumAccess(ident, variant_name));
                        } else {
                            return Err(format!("Expected enum variant after '::' at line {}:{}",
                                              self.current_token.line, self.current_token.column));
                        }
                    } else {
                        return Ok(Expr::Variable(ident));
                    }
                }

                match &self.current_token.token_type {
                    TokenType::LParen => {
                        self.advance();
                        let mut args = Vec::new();
                        if !matches!(self.current_token.token_type, TokenType::RParen) {
                            loop {
                                args.push(self.parse_expression()?);
                                match &self.current_token.token_type {
                                    TokenType::Comma => {
                                        self.advance();
                                        continue;
                                    }
                                    TokenType::RParen => break,
                                    _ => return Err(format!("Expected ',' or ')' at line {}:{}",
                                                          self.current_token.line, self.current_token.column)),
                                }
                            }
                        }
                        self.expect(TokenType::RParen)?;
                        Ok(Expr::Call(ident, args))
                    }
                    TokenType::LBracket => {
                        self.advance();
                        let index = self.parse_expression()?;
                        self.expect(TokenType::RBracket)?;
                        Ok(Expr::ArrayIndex(Box::new(Expr::Variable(ident)), Box::new(index)))
                    }
                    _ => Ok(Expr::Variable(ident)),
                }
            }
            TokenType::LBracket => {
                self.advance();
                let mut elements = Vec::new();
                if !matches!(self.current_token.token_type, TokenType::RBracket) {
                    loop {
                        elements.push(self.parse_expression()?);
                        match &self.current_token.token_type {
                            TokenType::Comma => {
                                self.advance();
                                continue;
                            }
                            TokenType::RBracket => break,
                            _ => return Err(format!("Expected ',' or ']' at line {}:{}",
                                                  self.current_token.line, self.current_token.column)),
                        }
                    }
                }
                self.expect(TokenType::RBracket)?;
                Ok(Expr::Array(elements))
            }
            TokenType::LParen => {
                self.advance();

                let mut args = Vec::new();
                if !matches!(self.current_token.token_type, TokenType::RParen) {
                    loop {
                        args.push(self.parse_expression()?);
                        match &self.current_token.token_type {
                            TokenType::Comma => {
                                self.advance();
                                continue;
                            }
                            TokenType::RParen => break,
                            _ => return Err(format!("Expected ',' or ')' at line {}:{}",
                                                  self.current_token.line, self.current_token.column)),
                        }
                    }
                }
                self.expect(TokenType::RParen)?;

                if let TokenType::Identifier(name) = &self.current_token.token_type {
                    let func_name = name.clone();
                    self.advance();

                    return Ok(Expr::Call(func_name, args));
                } else if args.len() == 1 {
                    return Ok(args[0].clone());
                } else {
                    return Err(format!("Expected function name after ')' at line {}:{}",
                                      self.current_token.line, self.current_token.column));
                }
            }
            _ => Err(format!("Unexpected token {} at line {}:{}",
                            self.current_token.token_type,
                            self.current_token.line,
                            self.current_token.column)),
        }
    }
}