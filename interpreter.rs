#![allow(dead_code)]
use crate::sha256sum::Sha256;
use crate::sha512sum::Sha512;
use crate::parser::{Expr, Literal, Statement, BinaryOp, UnaryOp};
use std::collections::HashMap;
use std::cell::RefCell;
use std::rc::Rc;
use std::path::Path;
use std::fmt;
use std::io::{Read};
use lexer::Lexer;
use parser::Parser;
use std::collections::HashSet;
use std::os::raw::c_char;

type BuiltinFn = fn(&mut Interpreter, Vec<Expr>) -> Result<Value, String>;

const REG_ICASE: i32 = 0x0002;
const REG_NEWLINE: i32 = 0x0010;
const REG_EXTENDED: i32 = 0x0001;

#[cfg(gui)]
static mut CALLBACK_INTERPRETER: *mut Interpreter = std::ptr::null_mut();

#[cfg(gui)]
extern "C" fn callback_bridge(name: *const i8) {
    unsafe {
        if !CALLBACK_INTERPRETER.is_null() {
            let interpreter = &mut *CALLBACK_INTERPRETER;
            let c_str = std::ffi::CStr::from_ptr(name as *const c_char);
            let func_name = c_str.to_string_lossy().into_owned();

            if let Some(func) = interpreter.callbacks.get(&func_name) {
                if let Value::Function(f) = func {
                    println!("Callback triggered: {}", func_name);
                }
            }
        }
    }
}

#[cfg(gui)]
static mut GLOBAL_INTERPRETER: *mut Interpreter = std::ptr::null_mut();

#[cfg(gui)]
extern "C" fn button_callback_handler(callback_id: *const i8) {
    unsafe {
        if callback_id.is_null() || GLOBAL_INTERPRETER.is_null() {
            return;
        }

        let interpreter = &mut *GLOBAL_INTERPRETER;

        let c_str = match std::ffi::CStr::from_ptr(callback_id as *const c_char).to_str() {
            Ok(s) => s,
            Err(_) => return,
        };

        if let Some(callback_str) = interpreter.button_callbacks.get(c_str).cloned() {
            let _ = interpreter.execute_callback(&callback_str);
        }
    }
}
#[derive(Debug, Clone)]
pub enum Value {
    Number(f64),
    String(String),
    Char(char),
    Boolean(bool),
    Array(Vec<Value>),
    Function(Rc<Function>),
    FileHandle(usize),
    Enum(String, String),
    DatabaseHandle(usize),
    HashMap(HashMap<Value, Value>),
    Struct(String, HashMap<String, Value>),
    GUIFrame(*mut std::ffi::c_void),
    GUIWidget(*mut std::ffi::c_void),
    Nil,
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Number(a), Value::Number(b)) => {
                if a.is_nan() && b.is_nan() {
                    return false;
                }
                a == b
            }
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Char(a), Value::Char(b)) => a == b,
            (Value::Boolean(a), Value::Boolean(b)) => a == b,
            (Value::Array(a), Value::Array(b)) => a == b,
            (Value::DatabaseHandle(a), Value::DatabaseHandle(b)) => a == b,
            (Value::HashMap(a), Value::HashMap(b)) => {
                if a.len() != b.len() {
                    return false;
                }
                for (k, v) in a.iter() {
                    match b.get(k) {
                        Some(bv) => {
                            if v != bv {
                                return false;
                            }
                        }
                        None => return false,
                    }
                }
                true
            }
            (Value::Struct(_, a_fields), Value::Struct(_, b_fields)) => a_fields == b_fields,
            (Value::Function(a), Value::Function(b)) => {
                std::ptr::eq(a.as_ref(), b.as_ref())
            }
            (Value::FileHandle(a), Value::FileHandle(b)) => a == b,
            (Value::Enum(a_name, a_variant), Value::Enum(b_name, b_variant)) => {
                a_name == b_name && a_variant == b_variant
            }
            (Value::Nil, Value::Nil) => true,
            _ => false,
        }
    }
}

impl Eq for Value {}
use std::hash::{Hash, Hasher};

impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Value::Number(n) => {
                if n.is_nan() {
                    0u8.hash(state);
                } else {
                    1u8.hash(state);
                    n.to_bits().hash(state);
                }
            }
            Value::String(s) => {
                2u8.hash(state);
                s.hash(state);
            }
            Value::Char(c) => {
                3u8.hash(state);
                c.hash(state);
            }
            Value::Boolean(b) => {
                4u8.hash(state);
                b.hash(state);
            }
            Value::Array(arr) => {
                5u8.hash(state);
                arr.hash(state);
            }
            Value::HashMap(map) => {
                6u8.hash(state);
                map.len().hash(state);
                let mut pairs: Vec<_> = map.iter().collect();
                pairs.sort_by(|a, b| {
                    let a_key_str = a.0.to_string();
                    let b_key_str = b.0.to_string();
                    a_key_str.cmp(&b_key_str)
                });
                for (k, v) in pairs {
                    k.hash(state);
                    v.hash(state);
                }
            }
            Value::GUIFrame(ptr) => {
                12u8.hash(state);
                (*ptr as usize).hash(state);
            }
            Value::GUIWidget(ptr) => {
                13u8.hash(state);
                (*ptr as usize).hash(state);
            }
            Value::Struct(name, fields) => {
                7u8.hash(state);
                name.hash(state);
                fields.len().hash(state);
                let mut sorted_fields: Vec<_> = fields.iter().collect();
                sorted_fields.sort_by(|a, b| a.0.cmp(b.0));
                for (k, v) in sorted_fields {
                    k.hash(state);
                    v.hash(state);
                }
            }
            Value::Function(_) => {
                8u8.hash(state);
                std::ptr::hash(self, state);
            }
            Value::FileHandle(h) => {
                9u8.hash(state);
                h.hash(state);
            }
            Value::Enum(name, variant) => {
                10u8.hash(state);
                name.hash(state);
                variant.hash(state);
            }
            Value::DatabaseHandle(h) => {
                15u8.hash(state);
                h.hash(state);
            }
            Value::Nil => {
                11u8.hash(state);
            }
        }
    }
}
impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Value::Number(n) => write!(f, "{}", n),
            Value::String(s) => write!(f, "{}", s),
            Value::Char(c) => write!(f, "{}", c),
            Value::Boolean(b) => write!(f, "{}", b),
            Value::GUIFrame(_) => write!(f, "<GUI Frame>"),
            Value::GUIWidget(_) => write!(f, "<GUI Widget>"),
            Value::Array(arr) => {
                write!(f, "[")?;
                for (i, v) in arr.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", v)?;
                }
                write!(f, "]")
            }
            Value::HashMap(map) => {
                write!(f, "{{")?;
                let mut first = true;
                for (key, value) in map {
                    if !first {
                        write!(f, ", ")?;
                    }

                    match key {
                        Value::String(s) => write!(f, "\"{}\": ", s)?,
                        Value::Char(c) => write!(f, "'{}': ", c)?,
                        Value::Number(n) => write!(f, "{}: ", n)?,
                        Value::Boolean(b) => write!(f, "{}: ", b)?,
                        Value::Array(arr) => write!(f, "{}: ", Value::Array(arr.clone()))?,
                        Value::HashMap(submap) => write!(f, "{}: ", Value::HashMap(submap.clone()))?,
                        Value::Enum(name, variant) => write!(f, "{}::{}: ", name, variant)?,
                        Value::Nil => write!(f, "nil: ")?,
                        _ => write!(f, "{:?}: ", key)?,
                    }

                    match value {
                        Value::String(s) => write!(f, "\"{}\"", s)?,
                        Value::Char(c) => write!(f, "'{}'", c)?,
                        _ => write!(f, "{}", value)?,
                    }
                    first = false;
                }
                write!(f, "}}")
            }
            Value::Function(_) => write!(f, "<function>"),
            Value::FileHandle(h) => write!(f, "<file {}>", h),
            Value::Struct(name, fields) => {
                write!(f, "{} {{ ", name)?;
                let mut first = true;
                for (key, value) in fields {
                    if !first {
                        write!(f, ", ")?;
                    }
                    write!(f, "${}: {}", key, value)?;
                    first = false;
                }
                write!(f, " }}")
            }
            Value::Enum(enum_name, variant) => write!(f, "{}::{}", enum_name, variant),
            Value::DatabaseHandle(h) => write!(f, "<database {}>", h),
            Value::Nil => write!(f, "nil"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub params: Vec<String>,
    pub body: Vec<Statement>,
    pub param_count: usize,
    pub is_private: bool,
    pub closure_env: Option<Rc<RefCell<Environment>>>,
}

impl PartialEq for Function {
    fn eq(&self, _other: &Self) -> bool {
        false
    }
}

#[derive(Debug, Clone)]
pub struct Environment {
    values: HashMap<String, Value>,
    functions: HashMap<String, Vec<Rc<Function>>>,
    enums: HashMap<String, Vec<String>>,
    parent: Option<Rc<RefCell<Environment>>>,
    is_global: bool,
    private_values: HashMap<String, Value>,
    private_functions: HashMap<String, Vec<Rc<Function>>>,
}

impl Environment {
    pub fn new() -> Self {
        Environment {
            values: HashMap::new(),
            parent: None,
            functions: HashMap::new(),
            enums: HashMap::new(),
            is_global: true,
            private_values: HashMap::new(),
            private_functions: HashMap::new(),
        }
    }

    pub fn with_parent(parent: Rc<RefCell<Environment>>) -> Self {
        Environment {
            values: HashMap::new(),
            enums: HashMap::new(),
            parent: Some(parent),
            is_global: false,
            functions: HashMap::new(),
            private_values: HashMap::new(),
            private_functions: HashMap::new(),
        }
    }

    pub fn define_private(&mut self, name: String, value: Value) {
        self.private_values.insert(name, value);
    }

    pub fn define_private_function(&mut self, name: String, function: Rc<Function>) {
        self.private_functions
            .entry(name)
            .or_insert_with(Vec::new)
            .push(function);
    }

    pub fn get_private(&self, name: &str) -> Option<Value> {
        self.private_values.get(name).cloned()
    }

    pub fn get_private_function(&self, name: &str, arg_count: usize) -> Option<Rc<Function>> {
        if let Some(functions) = self.private_functions.get(name) {
            for func in functions {
                if func.params.len() == arg_count {
                    return Some(func.clone());
                }
            }
        }
        None
    }

    pub fn define(&mut self, name: String, value: Value) {
        self.values.insert(name, value);
    }

    pub fn define_function(&mut self, name: String, function: Rc<Function>) {
        self.functions
            .entry(name)
            .or_insert_with(Vec::new)
            .push(function);
    }

    pub fn define_enum(&mut self, name: String, variants: Vec<String>) {
        self.enums.insert(name, variants);
    }

    pub fn get_enum_variant(&self, enum_name: &str, variant_name: &str) -> Option<Value> {
        if let Some(variants) = self.enums.get(enum_name) {
            if variants.contains(&variant_name.to_string()) {
                return Some(Value::Enum(enum_name.to_string(), variant_name.to_string()));
            }
        }

        if let Some(parent) = &self.parent {
            return parent.borrow().get_enum_variant(enum_name, variant_name);
        }

        None
    }

    pub fn get_function(&self, name: &str, arg_count: usize) -> Option<Rc<Function>> {
        if let Some(functions) = self.functions.get(name) {
            for func in functions {
                if func.params.len() == arg_count {
                    return Some(func.clone());
                }
            }
        }

        if let Some(parent) = &self.parent {
            return parent.borrow().get_function(name, arg_count);
        }

        None
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        if let Some(value) = self.private_values.get(name) {
            return Some(value.clone());
        }

        if let Some(value) = self.values.get(name) {
            return Some(value.clone());
        }

        if let Some(parent) = &self.parent {
            return parent.borrow().get(name);
        }

        None
    }

    pub fn set(&mut self, name: String, value: Value) -> Result<(), String> {
        if self.values.contains_key(&name) {
            self.values.insert(name, value);
            Ok(())
        } else if let Some(parent) = &mut self.parent {
            parent.borrow_mut().set(name, value)
        } else {
            Err(format!("Undefined variable: {}", name))
        }
    }

    pub fn get_global(&self, name: &str) -> Option<Value> {
        if self.is_global {
            self.values.get(name).cloned()
        } else if let Some(parent) = &self.parent {
            parent.borrow().get_global(name)
        } else {
            None
        }
    }

    pub fn set_global(&mut self, name: String, value: Value) -> Result<(), String> {
        if self.is_global {
            if self.values.contains_key(&name) {
                self.values.insert(name, value);
                Ok(())
            } else {
                Err(format!("Global variable '{}' not found", name))
            }
        } else if let Some(parent) = &mut self.parent {
            parent.borrow_mut().set_global(name, value)
        } else {
            Err(format!("Global variable '{}' not found", name))
        }
    }

    pub fn get_parent_scope(&self, name: &str) -> Option<Value> {
        if let Some(value) = self.values.get(name) {
            return Some(value.clone());
        }
        if let Some(parent) = &self.parent {
            return parent.borrow().get_parent_scope(name);
        }
        None
    }

    pub fn set_parent_scope(&mut self, name: String, value: Value) -> Result<(), String> {
        if self.values.contains_key(&name) {
            self.values.insert(name, value);
            return Ok(());
        }

        if let Some(parent) = &mut self.parent {
            return parent.borrow_mut().set_parent_scope(name, value);
        }
        Err(format!("Variable '{}' not found in any scope", name))
    }

    pub fn assign(&mut self, name: String, value: Value) -> Result<(), String> {
        if self.values.contains_key(&name) {
            self.values.insert(name, value);
            Ok(())
        } else if let Some(parent) = &mut self.parent {
            parent.borrow_mut().assign(name, value)
        } else {
            Err(format!("Variable {} not found for reassignment", name))
        }
    }
}

pub struct Interpreter {
    environment: Rc<RefCell<Environment>>,
    return_value: Option<Value>,
    should_break: bool,
    should_continue: bool,
    server_output: String,
    is_server_mode: bool,
    form_data: HashMap<String, String>,
    request_method: String,
   databases: Vec<Option<*mut std::ffi::c_void>>,
    terminal_args: Vec<String>,
    files: Vec<Option<std::fs::File>>,
    pub callbacks: HashMap<String, Value>,
    struct_counter: usize,
    loaded_modules: HashMap<String, Rc<RefCell<Environment>>>,
    module_aliases: HashMap<String, String>,
    alias_to_module: HashMap<String, String>,
    module_contents: HashMap<String, String>,
    loading_modules: HashSet<String>,
    aliased_modules: HashSet<String>,
    pub button_callbacks: HashMap<String, String>,
    callback_counter: usize,
    pub caught_error: Option<Value>,
}

impl Interpreter {
    pub fn new() -> Self {
        let env = Rc::new(RefCell::new(Environment::new()));

        {
            let mut env_mut = env.borrow_mut();
            env_mut.define("SEEK_SET".to_string(), Value::Number(0.0));
            env_mut.define("SEEK_CUR".to_string(), Value::Number(1.0));
            env_mut.define("SEEK_END".to_string(), Value::Number(2.0));
        }

        let interpreter = Interpreter {
            environment: env.clone(),
            return_value: None,
            should_break: false,
            should_continue: false,
            server_output: String::new(),
            is_server_mode: false,
            form_data: HashMap::new(),
            request_method: String::new(),
            terminal_args: Vec::new(),
            files: Vec::new(),
            databases: Vec::new(),
            struct_counter: 0,
            callbacks: HashMap::new(),
            loaded_modules: HashMap::new(),
            module_aliases: HashMap::new(),
            alias_to_module: HashMap::new(),
            module_contents: HashMap::new(),
            loading_modules: HashSet::new(),
            aliased_modules: HashSet::new(),
            button_callbacks: HashMap::new(),
            callback_counter: 0,
            caught_error: None,
        };

        interpreter
    }

    pub fn with_server_output() -> Self {
        let env = Rc::new(RefCell::new(Environment::new()));
        {
            let mut env_mut = env.borrow_mut();
            env_mut.define("SEEK_SET".to_string(), Value::Number(0.0));
            env_mut.define("SEEK_CUR".to_string(), Value::Number(1.0));
            env_mut.define("SEEK_END".to_string(), Value::Number(2.0));
        }
        Interpreter {
            environment: env.clone(),
            return_value: None,
            should_break: false,
            should_continue: false,
            server_output: String::new(),
            is_server_mode: true,
            form_data: HashMap::new(),
            request_method: String::new(),
            terminal_args: Vec::new(),
            files: Vec::new(),
            databases: Vec::new(),
            struct_counter: 0,
            callbacks: HashMap::new(),
            loaded_modules: HashMap::new(),
            module_aliases: HashMap::new(),
            alias_to_module: HashMap::new(),
            module_contents: HashMap::new(),
            loading_modules: HashSet::new(),
            aliased_modules: HashSet::new(),
            button_callbacks: HashMap::new(),
            callback_counter: 0,
            caught_error: None,
        }
    }

    pub fn with_server_data(form_data: &HashMap<String, String>, method: &str) -> Self {
        let env = Rc::new(RefCell::new(Environment::new()));
        {
            let mut env_mut = env.borrow_mut();
            env_mut.define("SEEK_SET".to_string(), Value::Number(0.0));
            env_mut.define("SEEK_CUR".to_string(), Value::Number(1.0));
            env_mut.define("SEEK_END".to_string(), Value::Number(2.0));
        }
        Interpreter {
            environment: env.clone(),
            return_value: None,
            should_break: false,
            should_continue: false,
            server_output: String::new(),
            is_server_mode: true,
            form_data: form_data.clone(),
            request_method: method.to_string(),
            terminal_args: Vec::new(),
            files: Vec::new(),
            databases: Vec::new(),
            struct_counter: 0,
            callbacks: HashMap::new(),
            loaded_modules: HashMap::new(),
            module_aliases: HashMap::new(),
            alias_to_module: HashMap::new(),
            module_contents: HashMap::new(),
            loading_modules: HashSet::new(),
            aliased_modules: HashSet::new(),
            button_callbacks: HashMap::new(),
            callback_counter: 0,
            caught_error: None,
        }
    }

    pub fn with_terminal_args(args: Vec<String>) -> Self {
        let env = Rc::new(RefCell::new(Environment::new()));
        {
            let mut env_mut = env.borrow_mut();
            env_mut.define("SEEK_SET".to_string(), Value::Number(0.0));
            env_mut.define("SEEK_CUR".to_string(), Value::Number(1.0));
            env_mut.define("SEEK_END".to_string(), Value::Number(2.0));
        }
        Interpreter {
            environment: env.clone(),
            return_value: None,
            should_break: false,
            should_continue: false,
            server_output: String::new(),
            is_server_mode: false,
            form_data: HashMap::new(),
            request_method: String::new(),
            terminal_args: args,
            files: Vec::new(),
            databases: Vec::new(),
            callbacks: HashMap::new(),
            struct_counter: 0,
            loaded_modules: HashMap::new(),
            module_aliases: HashMap::new(),
            alias_to_module: HashMap::new(),
            module_contents: HashMap::new(),
            loading_modules: HashSet::new(),
            aliased_modules: HashSet::new(),
            button_callbacks: HashMap::new(),
            callback_counter: 0,
            caught_error: None,
        }
    }

    pub fn get_server_output(&self) -> String {
        self.server_output.clone()
    }

    pub fn clear_server_output(&mut self) {
        self.server_output.clear();
    }
    fn builtin_print_s(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("print_s expects exactly one argument".to_string());
        }
        let value = interpreter.evaluate_expression(args[0].clone())?;

        if interpreter.is_server_mode {
            interpreter.server_output.push_str(&value.to_string());
        } else {
            println!("{}", value);
        }
        Ok(Value::Nil)
    }

    fn parse_callback_string(&self, callback_str: &str) -> Result<(String, Vec<String>), String> {
        let paren_pos = callback_str.find('(')
            .ok_or_else(|| format!("Invalid callback format: '{}' - missing '('", callback_str))?;

        let func_name = callback_str[0..paren_pos].trim().to_string();
        if func_name.is_empty() {
            return Err(format!("Invalid callback format: '{}' - missing function name", callback_str));
        }

        let args_part = callback_str[paren_pos+1..].trim();
        if !args_part.ends_with(')') {
            return Err(format!("Invalid callback format: '{}' - missing ')'", callback_str));
        }

        let args_str = &args_part[0..args_part.len()-1];

        let mut args = Vec::new();
        if !args_str.is_empty() {
            for arg in args_str.split(',').map(|s| s.trim()) {
                if !arg.is_empty() {
                    args.push(arg.to_string());
                }
            }
        }

        Ok((func_name, args))
    }

    fn execute_callback(&mut self, callback_str: &str) -> Result<Value, String> {
        let (func_name, arg_strings) = self.parse_callback_string(callback_str)?;

        let mut evaluated_args = Vec::new();
        for arg_str in arg_strings {
            let value = self.evaluate_callback_arg(&arg_str)?;
            evaluated_args.push(Expr::Literal(self.value_to_literal(value)?));
        }

        let call_expr = Expr::Call(func_name, evaluated_args);
        self.evaluate_expression(call_expr)
    }

    fn value_to_literal(&self, value: Value) -> Result<Literal, String> {
        match value {
            Value::Number(n) => Ok(Literal::Number(n)),
            Value::String(s) => Ok(Literal::String(s)),
            Value::Char(c) => Ok(Literal::Char(c)),
            Value::Boolean(b) => Ok(Literal::Boolean(b)),
            Value::Nil => Ok(Literal::Nil),
            _ => Err(format!("Cannot convert {:?} to literal", value)),
        }
    }

    fn evaluate_callback_arg(&self, arg_str: &str) -> Result<Value, String> {
        let trimmed = arg_str.trim();

        if trimmed.starts_with('$') {
            let var_name = &trimmed[1..];
            return self.environment.borrow().get(var_name)
                .ok_or_else(|| format!("Variable '{}' not found", var_name));
        }

        if trimmed.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '-') {
            if let Ok(num) = trimmed.parse::<f64>() {
                return Ok(Value::Number(num));
            }
        }

        if trimmed == "true" {
            return Ok(Value::Boolean(true));
        }
        if trimmed == "false" {
            return Ok(Value::Boolean(false));
        }

        if trimmed == "nil" {
            return Ok(Value::Nil);
        }

        if (trimmed.starts_with('"') && trimmed.ends_with('"')) ||
           (trimmed.starts_with('\'') && trimmed.ends_with('\'')) {
            return Ok(Value::String(trimmed[1..trimmed.len()-1].to_string()));
        }

        if let Ok(num) = trimmed.parse::<f64>() {
            return Ok(Value::Number(num));
        }

        Ok(Value::String(trimmed.to_string()))
    }
    pub fn interpret(&mut self, statements: Vec<Statement>) -> Result<(), String> {
        for stmt in statements {
            self.execute_statement(stmt)?;
            if self.return_value.is_some() {
                break;
            }
        }
        Ok(())
    }

    fn get_builtin_function(&self, name: &str) -> Option<BuiltinFn> {
        match name {
            "print" => Some(Self::builtin_print),
            "in" => Some(Self::builtin_in),
            "print_s" => Some(Self::builtin_print_s),
            "time" => Some(Self::builtin_time),
            "rand" => Some(Self::builtin_rand),
            "rand_float" => Some(Self::builtin_rand_float),
            "sleep" => Some(Self::builtin_sleep),
            "exit" => Some(Self::builtin_exit),
            "system" => Some(Self::builtin_system),
            "array" => Some(Self::builtin_array),
            "array_generator" => Some(Self::builtin_array_generator),
            "range" => Some(Self::builtin_range),
            "uuid_v4" => Some(Self::builtin_uuid_v4),
            "base64_encode" => Some(Self::builtin_base64_encode),
            "base64_decode" => Some(Self::builtin_base64_decode),
            "min" => Some(Self::builtin_min),
            "max" => Some(Self::builtin_max),
            "date" => Some(Self::builtin_date),
            "eval" => Some(Self::builtin_eval),
            "var_dump" => Some(Self::builtin_var_dump),
            "global" => Some(Self::builtin_global),
            "global_set" => Some(Self::builtin_global_set),
            "parent" => Some(Self::builtin_parent),
            "parent_set" => Some(Self::builtin_parent_set),
            "getenv" => Some(Self::builtin_getenv),
            "setenv" => Some(Self::builtin_setenv),
            "read_dir" => Some(Self::builtin_read_dir),
            "csv_write" => Some(Self::builtin_csv_write),
            "csv_read" => Some(Self::builtin_csv_read),
            "base" => Some(Self::builtin_base),
            "sin" => Some(Self::builtin_sin),
            "cos" => Some(Self::builtin_cos),
            "tan" => Some(Self::builtin_tan),
            "asin" => Some(Self::builtin_asin),
            "acos" => Some(Self::builtin_acos),
            "atan" => Some(Self::builtin_atan),
            "atan2" => Some(Self::builtin_atan2),
            "csc" => Some(Self::builtin_csc),
            "sec" => Some(Self::builtin_sec),
            "cot" => Some(Self::builtin_cot),
            "sinh" => Some(Self::builtin_sinh),
            "cosh" => Some(Self::builtin_cosh),
            "tanh" => Some(Self::builtin_tanh),
            "asinh" => Some(Self::builtin_asinh),
            "acosh" => Some(Self::builtin_acosh),
            "atanh" => Some(Self::builtin_atanh),
            "exp" => Some(Self::builtin_exp),
            "log" => Some(Self::builtin_log),
            "log10" => Some(Self::builtin_log10),
            "log2" => Some(Self::builtin_log2),
            "pow" => Some(Self::builtin_pow),
            "sqrt" => Some(Self::builtin_sqrt),
            "cbrt" => Some(Self::builtin_cbrt),
            "hypot" => Some(Self::builtin_hypot),
            "factorial" => Some(Self::builtin_factorial),
            "permutation" => Some(Self::builtin_permutation),
            "combination" => Some(Self::builtin_combination),
            "gcd" => Some(Self::builtin_gcd),
            "lcm" => Some(Self::builtin_lcm),
            "abs" => Some(Self::builtin_abs),
            "ceil" => Some(Self::builtin_ceil),
            "floor" => Some(Self::builtin_floor),
            "round" => Some(Self::builtin_round),
            "trunc" => Some(Self::builtin_trunc),
            "erf" => Some(Self::builtin_erf),
            "erfc" => Some(Self::builtin_erfc),
            "gamma" => Some(Self::builtin_gamma),
            "pi" => Some(Self::builtin_pi),
            "e" => Some(Self::builtin_e),
            "array_push" => Some(Self::builtin_array_push),
            "array_pop" => Some(Self::builtin_array_pop),
            "array_shift" => Some(Self::builtin_array_shift),
            "array_unshift" => Some(Self::builtin_array_unshift),
            "array_get" => Some(Self::builtin_array_get),
            "array_set" => Some(Self::builtin_array_set),
            "array_len" => Some(Self::builtin_array_len),
            "array_remove" => Some(Self::builtin_array_remove),
            "array_insert" => Some(Self::builtin_array_insert),
            "array_contains" => Some(Self::builtin_array_contains),
            "array_index_of" => Some(Self::builtin_array_index_of),
            "array_last_index_of" => Some(Self::builtin_array_last_index_of),
            "array_min" => Some(Self::builtin_array_min),
            "array_max" => Some(Self::builtin_array_max),
            "array_rand" => Some(Self::builtin_array_rand),
            "array_join" => Some(Self::builtin_array_join),
            "array_slice" => Some(Self::builtin_array_slice),
            "array_splice" => Some(Self::builtin_array_splice),
            "array_reverse" => Some(Self::builtin_array_reverse),
            "array_sort" => Some(Self::builtin_array_sort),
            "array_rsort" => Some(Self::builtin_array_rsort),
            "array_clear" => Some(Self::builtin_array_clear),
            "array_copy" => Some(Self::builtin_array_copy),
            "array_merge" => Some(Self::builtin_array_merge),
            "array_unique" => Some(Self::builtin_array_unique),
            "array_sum" => Some(Self::builtin_array_sum),
            "array_map" => Some(Self::builtin_array_map),
            "array_filter" => Some(Self::builtin_array_filter),
            "array_reduce" => Some(Self::builtin_array_reduce),
            "get" => Some(Self::builtin_get),
            "post" => Some(Self::builtin_post),
            "request_method" => Some(Self::builtin_request_method),
            "terminal" => Some(Self::builtin_terminal),
            "type_of_variable" => Some(Self::builtin_type_of_variable),
            "int" => Some(Self::builtin_int),
            "string" => Some(Self::builtin_string),
            "float" => Some(Self::builtin_float),
            "regex_test" => Some(Self::builtin_regex_test),
            "regex_find" => Some(Self::builtin_regex_find),
            "regex_replace" => Some(Self::builtin_regex_replace),
            "regex_split" => Some(Self::builtin_regex_split),
            "regex_escape" => Some(Self::builtin_regex_escape),
            "strlen" => Some(Self::builtin_strlen),
            "strcmp" => Some(Self::builtin_strcmp),
            "str_contains" => Some(Self::builtin_str_contains),
            "str_replace" => Some(Self::builtin_str_replace),
            "str_split" => Some(Self::builtin_str_split),
            "trim" => Some(Self::builtin_trim),
            "str_repeat" => Some(Self::builtin_str_repeat),
            "strpos" => Some(Self::builtin_strpos),
            "strtoupper" => Some(Self::builtin_strtoupper),
            "strtolower" => Some(Self::builtin_strtolower),
            "str_pad" => Some(Self::builtin_str_pad),
            "str_reverse" => Some(Self::builtin_str_reverse),
            "str_shuffle" => Some(Self::builtin_str_shuffle),
            "str_word_count" => Some(Self::builtin_str_word_count),
            "str_ends_with" => Some(Self::builtin_str_ends_with),
            "str_starts_with" => Some(Self::builtin_str_starts_with),
            "str_trim_left" => Some(Self::builtin_str_trim_left),
            "str_trim_right" => Some(Self::builtin_str_trim_right),
            "str_swapcase" => Some(Self::builtin_str_swapcase),
            "str_capitalize" => Some(Self::builtin_str_capitalize),
            "str_title" => Some(Self::builtin_str_title),
            "str_snake_case" => Some(Self::builtin_str_snake_case),
            "str_camel_case" => Some(Self::builtin_str_camel_case),
            "str_kebab_case" => Some(Self::builtin_str_kebab_case),
            "str_after" => Some(Self::builtin_str_after),
            "str_before" => Some(Self::builtin_str_before),
            "str_after_last" => Some(Self::builtin_str_after_last),
            "str_before_last" => Some(Self::builtin_str_before_last),
            "str_is_empty" => Some(Self::builtin_str_is_empty),
            "str_is_blank" => Some(Self::builtin_str_is_blank),
            "str_is_numeric" => Some(Self::builtin_str_is_numeric),
            "str_is_alpha" => Some(Self::builtin_str_is_alpha),
            "str_is_alphanumeric" => Some(Self::builtin_str_is_alphanumeric),
            "str_is_lowercase" => Some(Self::builtin_str_is_lowercase),
            "str_is_uppercase" => Some(Self::builtin_str_is_uppercase),
            "str_truncate" => Some(Self::builtin_str_truncate),
            "str_truncate_middle" => Some(Self::builtin_str_truncate_middle),
            "str_reverse_words" => Some(Self::builtin_str_reverse_words),
            "str_word_wrap" => Some(Self::builtin_str_word_wrap),
            "str_remove" => Some(Self::builtin_str_remove),
            "str_remove_all" => Some(Self::builtin_str_remove_all),
            "html_escape" => Some(Self::builtin_html_escape),
            "html_unescape" => Some(Self::builtin_html_unescape),
            "fopen" => Some(Self::builtin_fopen),
            "fclose" => Some(Self::builtin_fclose),
            "fwrite" => Some(Self::builtin_fwrite),
            "fwrite_line" => Some(Self::builtin_fwrite_line),
            "fread" => Some(Self::builtin_fread),
            "fread_line" => Some(Self::builtin_fread_line),
            "fread_lines" => Some(Self::builtin_fread_lines),
            "fseek" => Some(Self::builtin_fseek),
            "ftell" => Some(Self::builtin_ftell),
            "feof" => Some(Self::builtin_feof),
            "rewind" => Some(Self::builtin_rewind),
            "fflush" => Some(Self::builtin_fflush),
            "file_remove" => Some(Self::builtin_file_remove),
            "file_rename" => Some(Self::builtin_file_rename),
            "file_exists" => Some(Self::builtin_file_exists),
            "file_copy" => Some(Self::builtin_file_copy),
            "file_move" => Some(Self::builtin_file_move),
            "file_size" => Some(Self::builtin_file_size),
            "file_modified" => Some(Self::builtin_file_modified),
            "file_created" => Some(Self::builtin_file_created),
            "file_is_dir" => Some(Self::builtin_file_is_dir),
            "file_is_file" => Some(Self::builtin_file_is_file),
            "mkdir" => Some(Self::builtin_mkdir),
            "rmdir" => Some(Self::builtin_rmdir),
            "file_append" => Some(Self::builtin_file_append),
            "file_write_lines" => Some(Self::builtin_file_write_lines),
            "is_enum" => Some(Self::builtin_is_enum),
            "enum_name" => Some(Self::builtin_enum_name),
            "enum_variant" => Some(Self::builtin_enum_variant),
            "map_get_value" => Some(Self::builtin_map_get_value),
            "map_push" => Some(Self::builtin_map_push),
            "map_peek" => Some(Self::builtin_map_peek),
            "map_get_index" => Some(Self::builtin_map_get_index),
            "map_remove" => Some(Self::builtin_map_remove),
            "map_sort_as_key" => Some(Self::builtin_map_sort_as_key),
            "map_sort_as_value" => Some(Self::builtin_map_sort_as_value),
            "map_len" => Some(Self::builtin_map_len),
            "map_is_key_exists" => Some(Self::builtin_map_is_key_exists),
            "map_is_value_exists" => Some(Self::builtin_map_is_value_exists),
            "map_get_key" => Some(Self::builtin_map_get_key),
            "map_keys" => Some(Self::builtin_map_keys),
            "map_values" => Some(Self::builtin_map_values),
            "map_clear" => Some(Self::builtin_map_clear),
            "map_copy" => Some(Self::builtin_map_copy),
            "map_merge" => Some(Self::builtin_map_merge),
            "http_get" => Some(Self::builtin_http_get),
            "http_post" => Some(Self::builtin_http_post),
            "http_request" => Some(Self::builtin_http_request),
            "substr" => Some(Self::builtin_substr),
            "sha256_string" => Some(Self::builtin_sha256_string),
            "sha256_file" => Some(Self::builtin_sha256_file),
            "sha512_string" => Some(Self::builtin_sha512_string),
            "sha512_file" => Some(Self::builtin_sha512_file),
            "chr" => Some(Self::builtin_chr),
            "ord" => Some(Self::builtin_ord),
            "add" => Some(Self::builtin_add),
            "subtract" => Some(Self::builtin_subtract),
            "multiplication" => Some(Self::builtin_multiplication),
            "division" => Some(Self::builtin_division),
            "json_encode" => Some(Self::builtin_json_encode),
            "json_decode" => Some(Self::builtin_json_decode),
            "csv_to_json" => Some(Self::builtin_csv_to_json),
            "available_functions" => Some(Self::builtin_available_functions),
            "db_open" => Some(Self::builtin_db_open),
            "db_close" => Some(Self::builtin_db_close),
            "db_execute" => Some(Self::builtin_db_execute),
            "db_query" => Some(Self::builtin_db_query),
            "db_execute_params" => Some(Self::builtin_db_execute_params),
            "db_last_insert_id" => Some(Self::builtin_db_last_insert_id),
            "db_error" => Some(Self::builtin_db_error),
            #[cfg(gui)]
            "Frame" => Some(Self::builtin_frame),
            #[cfg(gui)]
            "Label" => Some(Self::builtin_label),
            #[cfg(gui)]
            "Button" => Some(Self::builtin_button),
            #[cfg(gui)]
            "button_on_click" => Some(Self::builtin_button_on_click),
            #[cfg(gui)]
            "auto_widget_scale" => Some(Self::builtin_auto_widget_scale),
            #[cfg(gui)]
            "gui_start" => Some(Self::builtin_gui_start),
            #[cfg(gui)]
            "gui_quit" => Some(Self::builtin_gui_quit),
            _ => None,
        }
    }

    #[cfg(gui)]
    fn builtin_frame(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        unsafe {
            GLOBAL_INTERPRETER = interpreter as *mut Interpreter;
            gui_set_callback(button_callback_handler);
        }
        if args.len() != 5 {
            return Err("Frame expects 5 arguments (title, width, height, x, y)".to_string());
        }
 
        let title = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("Frame: title must be a string".to_string()),
        };

        let width = match interpreter.evaluate_expression(args[1].clone())? {
            Value::Number(n) => n as i32,
            _ => return Err("Frame: width must be a number".to_string()),
        };

        let height = match interpreter.evaluate_expression(args[2].clone())? {
            Value::Number(n) => n as i32,
            _ => return Err("Frame: height must be a number".to_string()),
        };

        let x = match interpreter.evaluate_expression(args[3].clone())? {
            Value::Number(n) => n as i32,
            _ => return Err("Frame: x must be a number".to_string()),
        };

        let y = match interpreter.evaluate_expression(args[4].clone())? {
            Value::Number(n) => n as i32,
            _ => return Err("Frame: y must be a number".to_string()),
        };

        let c_title = std::ffi::CString::new(title)
            .map_err(|e| format!("Frame: invalid UTF-8 string: {}", e))?;

        let frame = unsafe {
            gui_frame_new(c_title.as_ptr() as *const i8, width, height, x, y)
        };

        Ok(Value::GUIFrame(frame))
    }

    #[cfg(gui)]
    fn builtin_label(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 5 {
            return Err("Label expects 5 arguments (frame, text, x, y, font_size)".to_string());
        }

        let frame = match interpreter.evaluate_expression(args[0].clone())? {
            Value::GUIFrame(f) => f,
            _ => return Err("Label: first argument must be a Frame".to_string()),
        };

        let text = match interpreter.evaluate_expression(args[1].clone())? {
            Value::String(s) => s,
            _ => return Err("Label: text must be a string".to_string()),
        };

        let x = match interpreter.evaluate_expression(args[2].clone())? {
            Value::Number(n) => n as i32,
            _ => return Err("Label: x must be a number".to_string()),
        };

        let y = match interpreter.evaluate_expression(args[3].clone())? {
            Value::Number(n) => n as i32,
            _ => return Err("Label: y must be a number".to_string()),
        };

        let font_size = match interpreter.evaluate_expression(args[4].clone())? {
            Value::Number(n) => n as i32,
            _ => return Err("Label: font_size must be a number".to_string()),
        };

        let c_text = std::ffi::CString::new(text)
            .map_err(|e| format!("Label: invalid UTF-8 string: {}", e))?;

        let label = unsafe {
            gui_label_new(frame, c_text.as_ptr() as *const i8, x, y, font_size)
        };

        Ok(Value::GUIWidget(label))
    }

    fn get_function(&self, name: &str, arg_count: usize) -> Option<Rc<Function>> {
        let env = self.environment.borrow();

        if let Some(func) = env.get_private_function(name, arg_count) {
            return Some(func);
        }

        if let Some(func) = env.get_function(name, arg_count) {
            return Some(func);
        }

        None
    }

    #[cfg(gui)]
    fn builtin_button(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 6 {
            return Err("Button expects 6 arguments (frame, text, x, y, width, height)".to_string());
        }

        let frame = match interpreter.evaluate_expression(args[0].clone())? {
            Value::GUIFrame(f) => f,
            _ => return Err("Button: first argument must be a Frame".to_string()),
        };

        let text = match interpreter.evaluate_expression(args[1].clone())? {
            Value::String(s) => s,
            _ => return Err("Button: text must be a string".to_string()),
        };

        let x = match interpreter.evaluate_expression(args[2].clone())? {
            Value::Number(n) => n as i32,
            _ => return Err("Button: x must be a number".to_string()),
        };

        let y = match interpreter.evaluate_expression(args[3].clone())? {
            Value::Number(n) => n as i32,
            _ => return Err("Button: y must be a number".to_string()),
        };

        let width = match interpreter.evaluate_expression(args[4].clone())? {
            Value::Number(n) => n as i32,
            _ => return Err("Button: width must be a number".to_string()),
        };

        let height = match interpreter.evaluate_expression(args[5].clone())? {
            Value::Number(n) => n as i32,
            _ => return Err("Button: height must be a number".to_string()),
        };

        let c_text = std::ffi::CString::new(text)
            .map_err(|e| format!("Button: invalid UTF-8 string: {}", e))?;

        let button = unsafe {
            gui_button_new(frame, c_text.as_ptr() as *const i8, x, y, width, height)
        };

        Ok(Value::GUIWidget(button))
    }

    #[cfg(gui)]
    fn builtin_button_on_click(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("button_on_click expects exactly 2 arguments (button, callback_string)".to_string());
        }

        let button = match interpreter.evaluate_expression(args[0].clone())? {
            Value::GUIWidget(w) => w,
            _ => return Err("button_on_click: first argument must be a button widget".to_string()),
        };

        let callback_str = match interpreter.evaluate_expression(args[1].clone())? {
            Value::String(s) => s,
            _ => return Err("button_on_click: second argument must be a string".to_string()),
        };

        let (func_name, _) = interpreter.parse_callback_string(&callback_str)?;

        {
            let env = interpreter.environment.borrow();
            if !env.functions.contains_key(&func_name) && env.get_function(&func_name, 0).is_none() {
                return Err(format!("Function '{}' not found for callback", func_name));
            }
        }

        let callback_id = format!("cb_{}", interpreter.callback_counter);
        interpreter.callback_counter += 1;

        interpreter.button_callbacks.insert(callback_id.clone(), callback_str);

        let c_callback_id = std::ffi::CString::new(callback_id)
            .map_err(|e| format!("Failed to create C string: {}", e))?;

        unsafe {
            gui_button_set_callback(button, c_callback_id.as_ptr() as *const i8);
        }

        Ok(Value::Nil)
    }

    #[cfg(gui)]
    fn builtin_auto_widget_scale(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("auto_widget_scale expects exactly 2 arguments (frame, enabled)".to_string());
        }

        let frame = match interpreter.evaluate_expression(args[0].clone())? {
            Value::GUIFrame(f) => f,
            _ => return Err("auto_widget_scale: first argument must be a Frame".to_string()),
        };

        let enabled = match interpreter.evaluate_expression(args[1].clone())? {
            Value::Boolean(b) => b,
            Value::Number(n) => n != 0.0,
            _ => return Err("auto_widget_scale: second argument must be a boolean or number".to_string()),
        };

        unsafe {
            gui_auto_widget_scale(frame, if enabled { 1 } else { 0 });
        }

        Ok(Value::Nil)
    }

    #[cfg(gui)]
    fn builtin_gui_start(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("gui_start expects exactly 1 argument (frame)".to_string());
        }

        let frame = match interpreter.evaluate_expression(args[0].clone())? {
            Value::GUIFrame(f) => f,
            _ => return Err("gui_start: argument must be a Frame".to_string()),
        };

        unsafe {
            gui_start(frame);
        }

        Ok(Value::Nil)
    }

    #[cfg(gui)]
    fn builtin_gui_quit(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if !args.is_empty() {
            return Err("gui_quit expects no arguments".to_string());
        }

        unsafe {
            gui_quit();
        }

        Ok(Value::Nil)
    }

    fn builtin_get(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("get expects exactly one argument (key)".to_string());
        }

        let key_val = interpreter.evaluate_expression(args[0].clone())?;
        let key = match key_val {
            Value::String(s) => s,
            _ => key_val.to_string(),
        };

        let value = interpreter.form_data.get(&key).unwrap_or(&"".to_string()).clone();
        Ok(Value::String(value))
    }

    fn builtin_post(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("post expects exactly one argument (key)".to_string());
        }

        let key_val = interpreter.evaluate_expression(args[0].clone())?;
        let key = match key_val {
            Value::String(s) => s,
            _ => key_val.to_string(),
        };

        let value = interpreter.form_data.get(&key).unwrap_or(&"".to_string()).clone();
        Ok(Value::String(value))
    }

    fn builtin_request_method(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if !args.is_empty() {
            return Err("request_method expects no arguments".to_string());
        }

        Ok(Value::String(interpreter.request_method.clone()))
    }

    fn builtin_print(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("print expects exactly one argument".to_string());
        }
        let value = interpreter.evaluate_expression(args[0].clone())?;
        println!("{}", value);
        Ok(Value::Nil)
    }

    fn builtin_date(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("date expects exactly 1 argument (format string)".to_string());
        }

        let format = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("date: format must be a string".to_string()),
        };

        let format_cstr = std::ffi::CString::new(format.as_str())
            .map_err(|e| format!("date: invalid format string: {}", e))?;

        let result_ptr = unsafe {
            date(format_cstr.as_ptr() as *const i8)
        };

        if result_ptr.is_null() {
            return Err("date: failed to format date".to_string());
        }

        let result_str = unsafe {
            let c_str = std::ffi::CStr::from_ptr(result_ptr as *const c_char);
            c_str.to_string_lossy().into_owned()
        };

        unsafe { free_string(result_ptr) };

        Ok(Value::String(result_str))
    }

    fn builtin_in(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() > 1 {
            return Err("in expects 0 or 1 argument (prompt message)".to_string());
        }

        if args.len() == 1 {
            let prompt = interpreter.evaluate_expression(args[0].clone())?;
            match prompt {
                Value::String(s) => print!("{}", s),
                _ => return Err("in: prompt must be a string".to_string()),
            }
        }

        use std::io::{self, Write};
        let mut input = String::new();

        io::stdout().flush().unwrap();

        match io::stdin().read_line(&mut input) {
            Ok(_) => {
                let trimmed = input.trim_end_matches('\n').trim_end_matches('\r');
                Ok(Value::String(trimmed.to_string()))
            }
            Err(e) => Err(format!("Failed to read input: {}", e)),
        }
    }
    fn builtin_exit(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("exit expects exactly 1 argument (status code)".to_string());
        }

        let status = match interpreter.evaluate_expression(args[0].clone())? {
            Value::Number(n) => n as i32,
            _ => return Err("exit: status code must be a number".to_string()),
        };

        unsafe {
            exit_program(status);
        }
    }

    fn builtin_array(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        let mut elements = Vec::new();

        for arg in args {
            let value = interpreter.evaluate_expression(arg)?;
            elements.push(value);
        }

        Ok(Value::Array(elements))
    }
    fn builtin_array_generator(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() < 2 || args.len() > 3 {
            return Err("array_generator expects 2-3 arguments (start, end, [filter])".to_string());
        }

        let start_val = interpreter.evaluate_expression(args[0].clone())?;
        let start = match start_val {
            Value::Number(n) => n as i64,
            _ => return Err("array_generator: start must be a number".to_string()),
        };

        let end_val = interpreter.evaluate_expression(args[1].clone())?;
        let end = match end_val {
            Value::Number(n) => n as i64,
            _ => return Err("array_generator: end must be a number".to_string()),
        };

        let has_filter = args.len() == 3;
        let filter_expr = if has_filter {
            Some(args[2].clone())
        } else {
            None
        };

        let mut result = Vec::new();
        let step = if start <= end { 1 } else { -1 };
        let mut current = start;

        if has_filter {
            while (step > 0 && current <= end) || (step < 0 && current >= end) {
                let old_env = interpreter.environment.clone();
                let new_env = Rc::new(RefCell::new(Environment::with_parent(old_env.clone())));
                interpreter.environment = new_env;

                interpreter.environment.borrow_mut().define("x".to_string(), Value::Number(current as f64));

                let filter_result = interpreter.evaluate_expression(filter_expr.clone().unwrap())?;
                let should_include = interpreter.is_truthy(&filter_result);

                interpreter.environment = old_env;

                if should_include {
                    result.push(Value::Number(current as f64));
                }

                current += step;
            }
        } else {
            while (step > 0 && current <= end) || (step < 0 && current >= end) {
                result.push(Value::Number(current as f64));
                current += step;
            }
        }

        Ok(Value::Array(result))
    }

    fn builtin_time(_interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if !args.is_empty() {
            return Err("time expects no arguments".to_string());
        }
        let timestamp = unsafe { get_time() };
        Ok(Value::Number(timestamp))
    }

    fn builtin_rand(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("rand expects exactly two arguments (min, max)".to_string());
        }

        let min_val = interpreter.evaluate_expression(args[0].clone())?;
        let max_val = interpreter.evaluate_expression(args[1].clone())?;

        match (min_val, max_val) {
            (Value::Number(min), Value::Number(max)) => {
                let random_num = unsafe { get_random(min as i32, max as i32) };
                Ok(Value::Number(random_num as f64))
            }
            _ => Err("rand expects numeric arguments".to_string()),
        }
    }

    fn builtin_rand_float(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if !args.is_empty() {
            return Err("rand_float expects no arguments".to_string());
        }

        use std::time::{SystemTime, UNIX_EPOCH};
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let time_seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        let thread_id = std::thread::current().id();
        let thread_hash = {
            let mut hasher = DefaultHasher::new();
            thread_id.hash(&mut hasher);
            hasher.finish()
        };

        let stack_var = 0;
        let stack_addr = &stack_var as *const i32 as u64;

        let mut seed = time_seed;
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed ^= thread_hash;
        seed ^= stack_addr;
        seed ^= 0x9e3779b97f4a7c15;
        seed ^= 0xbf58476d1ce4e5b9;

        if seed == 0 {
            seed = 0x123456789abcdef0;
        }

        let mut state = seed;
        let inc = (seed << 1) | 1;

        state = state.wrapping_mul(6364136223846793005).wrapping_add(inc);
        let mut x = state;

        x = x.wrapping_add(seed);
        x = x.wrapping_mul(0x9e3779b97f4a7c15);
        x = x.wrapping_add(thread_hash.wrapping_mul(0xbf58476d1ce4e5b9));
        x = x.wrapping_mul(0x94d049bb133111eb);
        x = x.wrapping_add(time_seed.wrapping_mul(0xd1b54a32d192ed03));

        x ^= x >> 30;
        x ^= x << 17;
        x ^= x >> 13;
        x ^= x << 7;
        x = x.wrapping_mul(0x9e3779b97f4a7c15);
        x ^= x >> 28;
        x ^= x << 15;
        x ^= x >> 25;
        x = x.wrapping_mul(0xbf58476d1ce4e5b9);
        x ^= x >> 31;
        x ^= x << 11;
        x ^= x >> 19;

        let mut result = x;
        result ^= result >> 33;
        result = result.wrapping_mul(0xff51afd7ed558ccd);
        result ^= result >> 33;
        result = result.wrapping_mul(0xc4ceb9fe1a85ec53);
        result ^= result >> 33;

        let mut state2 = seed ^ 0xdeadbeefcafebabe;
        state2 = state2.wrapping_mul(0x9e3779b97f4a7c15);
        state2 ^= state2 >> 27;
        state2 = state2.wrapping_mul(0xbf58476d1ce4e5b9);
        state2 ^= state2 >> 31;

        let combined = result ^ state2;
        let final_val = combined ^ (combined >> 32);

        let upper = 0xffffffffffffffffu64;
        let random_float = final_val as f64 / upper as f64;

        let random_float = if random_float == 0.0 || random_float == 1.0 {

            let flipped = final_val ^ 0x5a5a5a5a5a5a5a5a;
            flipped as f64 / upper as f64
        } else {
            random_float
        };

        Ok(Value::Number(random_float))
    }
    fn builtin_sleep(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("sleep expects exactly one argument (seconds)".to_string());
        }
        let val = interpreter.evaluate_expression(args[0].clone())?;

        match val {
            Value::Number(seconds) => {
                let seconds_int = seconds as i32;
                if seconds_int < 0 {
                    return Err("sleep argument must be non-negative".to_string());
                }

                let result = unsafe { sleep_seconds(seconds_int) };

                if result == 0 {
                    Ok(Value::Number(0.0))
                } else {
                    Err(format!("sleep failed with error code: {}", result))
                }
            }
            _ => Err("sleep expects a numeric argument".to_string()),
        }
    }

    fn builtin_global(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("global expects exactly one argument (variable name)".to_string());
        }

        match &args[0] {
            Expr::Variable(name) => {
                let value = interpreter.environment.borrow().get_global(name)
                    .ok_or_else(|| format!("Global variable '{}' not found", name))?;
                Ok(value)
            }
            _ => Err("global expects a variable name".to_string()),
        }
    }

    fn builtin_global_set(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("global_set expects exactly two arguments (variable name, value)".to_string());
        }

        match &args[0] {
            Expr::Variable(name) => {
                let value = interpreter.evaluate_expression(args[1].clone())?;
                interpreter.environment.borrow_mut().set_global(name.clone(), value)?;
                Ok(Value::Nil)
            }
            _ => Err("global_set expects a variable name as first argument".to_string()),
        }
    }

    fn builtin_parent(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("parent expects exactly one argument (variable name)".to_string());
        }

        match &args[0] {
            Expr::Variable(name) => {
                let value = interpreter.environment.borrow().get_parent_scope(name)
                    .ok_or_else(|| format!("Variable '{}' not found in any scope", name))?;
                Ok(value)
            }
            _ => Err("parent expects a variable name".to_string()),
        }
    }

    fn builtin_parent_set(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("parent_set expects exactly two arguments (variable name, value)".to_string());
        }

        match &args[0] {
            Expr::Variable(name) => {
                let value = interpreter.evaluate_expression(args[1].clone())?;
                interpreter.environment.borrow_mut().set_parent_scope(name.clone(), value)?;
                Ok(Value::Nil)
            }
            _ => Err("parent_set expects a variable name as first argument".to_string()),
        }
    }

    fn builtin_eval(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("eval expects exactly 1 argument (code string)".to_string());
        }

        let code = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("eval: argument must be a string".to_string()),
        };

        let mut isolated = Interpreter::new();

        let lexer = Lexer::new(&code);
        let mut parser = Parser::new(lexer);
        let ast = parser.parse_program()?;

        isolated.interpret(ast)?;

        Ok(isolated.return_value.unwrap_or(Value::Nil))
    }

    fn builtin_add(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("add expects exactly two arguments (a, b)".to_string());
        }
        let a_val = interpreter.evaluate_expression(args[0].clone())?;
        let b_val = interpreter.evaluate_expression(args[1].clone())?;
        match (a_val, b_val) {
            (Value::Number(a), Value::Number(b)) => {
                Ok(Value::Number(a + b))
            }
            _ => Err("add expects numeric arguments".to_string()),
        }
    }

    fn builtin_subtract(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("subtract expects exactly two arguments (a, b)".to_string());
        }
        let a_val = interpreter.evaluate_expression(args[0].clone())?;
        let b_val = interpreter.evaluate_expression(args[1].clone())?;
        match (a_val, b_val) {
            (Value::Number(a), Value::Number(b)) => {
                Ok(Value::Number(a - b))
            }
            _ => Err("subtract expects numeric arguments".to_string()),
        }
    }

    fn builtin_multiplication(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("multiplication expects exactly two arguments (a, b)".to_string());
        }
        let a_val = interpreter.evaluate_expression(args[0].clone())?;
        let b_val = interpreter.evaluate_expression(args[1].clone())?;
        match (a_val, b_val) {
            (Value::Number(a), Value::Number(b)) => {
                Ok(Value::Number(a * b))
            }
            _ => Err("multiplication expects numeric arguments".to_string()),
        }
    }

    fn builtin_division(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("division expects exactly two arguments (a, b)".to_string());
        }
        let a_val = interpreter.evaluate_expression(args[0].clone())?;
        let b_val = interpreter.evaluate_expression(args[1].clone())?;
        match (a_val, b_val) {
            (Value::Number(a), Value::Number(b)) => {
                if b == 0.0 {

                }
                Ok(Value::Number(a / b))
            }
            _ => Err("division expects numeric arguments".to_string()),
        }
    }

    fn builtin_json_encode(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("json_encode expects exactly 1 argument".to_string());
        }

        let value = interpreter.evaluate_expression(args[0].clone())?;
        let input = value.to_string();

        let result_ptr = unsafe {
            json_encode(input.as_ptr() as *const i8)
        };

        if result_ptr.is_null() {
            return Err("JSON encode failed".to_string());
        }

        let result_str = unsafe {
            let c_str = std::ffi::CStr::from_ptr(result_ptr as *const c_char);
            c_str.to_string_lossy().into_owned()
        };

        unsafe { free_string(result_ptr) };

        Ok(Value::String(result_str))
    }

    fn builtin_json_decode(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("json_decode expects exactly 1 argument".to_string());
        }

        let json_str = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("json_decode: argument must be a string".to_string()),
        };

        let result_ptr = unsafe {
            json_decode(json_str.as_ptr() as *const i8)
        };

        if result_ptr.is_null() {
            return Err("JSON decode failed".to_string());
        }

        let result_str = unsafe {
            let c_str = std::ffi::CStr::from_ptr(result_ptr as *const c_char);
            c_str.to_string_lossy().into_owned()
        };

        unsafe { free_string(result_ptr) };

        Self::parse_json_value(&result_str)
    }

    fn parse_json_value(json: &str) -> Result<Value, String> {
        let trimmed = json.trim();

        if trimmed.is_empty() {
            return Ok(Value::Nil);
        }

        if trimmed.starts_with('{') && trimmed.ends_with('}') {
            let inner = &trimmed[1..trimmed.len()-1].trim();
            let mut map = HashMap::new();

            if inner.is_empty() {
                return Ok(Value::HashMap(map));
            }

            let mut depth = 0;
            let mut in_string = false;
            let mut escaped = false;
            let mut start = 0;
            let mut pairs = Vec::new();
            let chars: Vec<char> = inner.chars().collect();

            for i in 0..chars.len() {
                let c = chars[i];

                if escaped {
                    escaped = false;
                    continue;
                }

                if c == '\\' {
                    escaped = true;
                    continue;
                }

                if c == '"' && !escaped {
                    in_string = !in_string;
                    continue;
                }

                if !in_string {
                    if c == '{' || c == '[' {
                        depth += 1;
                    } else if c == '}' || c == ']' {
                        depth -= 1;
                    } else if c == ',' && depth == 0 {
                        let pair = inner[start..i].trim();
                        if !pair.is_empty() {
                            pairs.push(pair.to_string());
                        }
                        start = i + 1;
                    }
                }
            }

            if start < inner.len() {
                let pair = inner[start..].trim();
                if !pair.is_empty() {
                    pairs.push(pair.to_string());
                }
            }

            for pair in pairs {
                if let Some(colon_pos) = pair.find(':') {
                    let key_str = pair[0..colon_pos].trim();
                    let value_str = pair[colon_pos+1..].trim();

                    let key = if key_str.starts_with('"') && key_str.ends_with('"') {
                        key_str[1..key_str.len()-1].to_string()
                    } else {
                        key_str.to_string()
                    };

                    let value = Self::parse_json_value(value_str)?;
                    map.insert(Value::String(key), value);
                }
            }

            return Ok(Value::HashMap(map));
        }

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let inner = &trimmed[1..trimmed.len()-1].trim();
            let mut arr = Vec::new();

            if inner.is_empty() {
                return Ok(Value::Array(arr));
            }

            let mut depth = 0;
            let mut in_string = false;
            let mut escaped = false;
            let mut start = 0;
            let chars: Vec<char> = inner.chars().collect();

            for i in 0..chars.len() {
                let c = chars[i];

                if escaped {
                    escaped = false;
                    continue;
                }

                if c == '\\' {
                    escaped = true;
                    continue;
                }

                if c == '"' && !escaped {
                    in_string = !in_string;
                    continue;
                }

                if !in_string {
                    if c == '{' || c == '[' {
                        depth += 1;
                    } else if c == '}' || c == ']' {
                        depth -= 1;
                    } else if c == ',' && depth == 0 {
                        let item = inner[start..i].trim();
                        if !item.is_empty() {
                            arr.push(Self::parse_json_value(item)?);
                        }
                        start = i + 1;
                    }
                }
            }

            if start < inner.len() {
                let item = inner[start..].trim();
                if !item.is_empty() {
                    arr.push(Self::parse_json_value(item)?);
                }
            }

            return Ok(Value::Array(arr));
        }

        if trimmed.starts_with('"') && trimmed.ends_with('"') {
            let s = trimmed[1..trimmed.len()-1].to_string();
            let mut unescaped = String::new();
            let mut chars = s.chars().peekable();
            while let Some(c) = chars.next() {
                if c == '\\' {
                    if let Some(next) = chars.next() {
                        match next {
                            'n' => unescaped.push('\n'),
                            't' => unescaped.push('\t'),
                            'r' => unescaped.push('\r'),
                            '"' => unescaped.push('"'),
                            '\\' => unescaped.push('\\'),
                            '/' => unescaped.push('/'),
                            _ => unescaped.push(next),
                        }
                    }
                } else {
                    unescaped.push(c);
                }
            }
            return Ok(Value::String(unescaped));
        }

        let is_number = trimmed.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '-');
        if is_number && !trimmed.is_empty() {
            if let Ok(num) = trimmed.parse::<f64>() {
                return Ok(Value::Number(num));
            }
        }

        if trimmed == "true" {
            return Ok(Value::Boolean(true));
        }
        if trimmed == "false" {
            return Ok(Value::Boolean(false));
        }

        if trimmed == "null" {
            return Ok(Value::Nil);
        }

        Ok(Value::String(trimmed.to_string()))
    }

    fn builtin_chr(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("chr expects exactly 1 argument (number)".to_string());
        }
    
        let n = match interpreter.evaluate_expression(args[0].clone())? {
            Value::Number(num) => num,
            _ => return Err("chr: argument must be a number".to_string()),
        };
    
        if n < 0.0 || n > 0x10FFFF as f64 || n.fract() != 0.0 {
            return Err("chr: argument must be a valid Unicode code point (0 to 1114111)".to_string());
        }
    
        match char::from_u32(n as u32) {
            Some(c) => Ok(Value::String(c.to_string())),
            None => Err(format!("chr: {} is not a valid Unicode code point", n as u32)),
        }
    }
    
    fn builtin_ord(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("ord expects exactly 1 argument (string)".to_string());
        }
    
        let s = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            Value::Char(c) => return Ok(Value::Number(c as u32 as f64)),
            _ => return Err("ord: argument must be a string or char".to_string()),
        };
    
        match s.chars().next() {
            Some(c) => Ok(Value::Number(c as u32 as f64)),
            None => Err("ord: string is empty".to_string()),
        }
    }

    fn builtin_available_functions(&mut self, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("available_functions expects exactly 1 argument (function_name)".to_string());
        }

        let func_name = match self.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("available_functions: argument must be a string".to_string()),
        };

        let mut functions: Vec<(&str, &str)> = vec![

            (
                "print",
                r#"{
                    "name": "print",
                    "description": "Prints a value to the console with a newline",
                    "parameters": [{"name": "value", "type": "any"}],
                    "return_type": "nil",
                    "example": "print(\"Hello World\")"
                }"#
            ),
            (
                "print_s",
                r#"{
                    "name": "print_s",
                    "description": "Prints a value (in server mode, outputs to HTML response)",
                    "parameters": [{"name": "value", "type": "any"}],
                    "return_type": "nil",
                    "example": "print_s(\"<h1>Hello</h1>\")"
                }"#
            ),
            (
                "in",
                r#"{
                    "name": "in",
                    "description": "Reads a line of input from the user with an optional prompt",
                    "parameters": [{"name": "prompt", "type": "string", "optional": true}],
                    "return_type": "string",
                    "example": "$name = in(\"Enter your name: \")"
                }"#
            ),

            (
                "int",
                r#"{
                    "name": "int",
                    "description": "Converts a value to an integer (truncates decimals)",
                    "parameters": [{"name": "value", "type": "any"}],
                    "return_type": "number (integer)",
                    "example": "int(\"123\") // returns 123"
                }"#
            ),
            (
                "float",
                r#"{
                    "name": "float",
                    "description": "Converts a value to a floating-point number",
                    "parameters": [{"name": "value", "type": "any"}],
                    "return_type": "number (float)",
                    "example": "float(\"3.14\") // returns 3.14"
                }"#
            ),
            (
                "string",
                r#"{
                    "name": "string",
                    "description": "Converts a value to a string",
                    "parameters": [{"name": "value", "type": "any"}],
                    "return_type": "string",
                    "example": "string(123) // returns \"123\""
                }"#
            ),
            (
                "type_of_variable",
                r#"{
                    "name": "type_of_variable",
                    "description": "Returns the type of a value as a string",
                    "parameters": [{"name": "value", "type": "any"}],
                    "return_type": "string",
                    "example": "type_of_variable(123) // returns \"int\""
                }"#
            ),

            (
                "strlen",
                r#"{
                    "name": "strlen",
                    "description": "Returns the length (number of characters) of a string",
                    "parameters": [{"name": "str", "type": "string"}],
                    "return_type": "number",
                    "example": "strlen(\"hello\") // returns 5"
                }"#
            ),
            (
                "strcmp",
                r#"{
                    "name": "strcmp",
                    "description": "Compares two strings lexicographically",
                    "parameters": [
                        {"name": "str1", "type": "string"},
                        {"name": "str2", "type": "string"}
                    ],
                    "return_type": "number (0 if equal, <0 if str1 < str2, >0 if str1 > str2)",
                    "example": "strcmp(\"abc\", \"abc\") // returns 0"
                }"#
            ),
            (
                "str_contains",
                r#"{
                    "name": "str_contains",
                    "description": "Checks if a string contains a substring",
                    "parameters": [
                        {"name": "haystack", "type": "string"},
                        {"name": "needle", "type": "string"}
                    ],
                    "return_type": "boolean",
                    "example": "str_contains(\"hello world\", \"world\") // returns true"
                }"#
            ),
            (
                "str_replace",
                r#"{
                    "name": "str_replace",
                    "description": "Replaces all occurrences of a search string with a replacement",
                    "parameters": [
                        {"name": "search", "type": "string"},
                        {"name": "replace", "type": "string"},
                        {"name": "subject", "type": "string"}
                    ],
                    "return_type": "string",
                    "example": "str_replace(\"world\", \"everyone\", \"hello world\") // returns \"hello everyone\""
                }"#
            ),
            (
                "str_split",
                r#"{
                    "name": "str_split",
                    "description": "Splits a string into an array of individual characters",
                    "parameters": [{"name": "str", "type": "string"}],
                    "return_type": "array of strings",
                    "example": "str_split(\"abc\") // returns [\"a\", \"b\", \"c\"]"
                }"#
            ),
            (
                "trim",
                r#"{
                    "name": "trim",
                    "description": "Removes whitespace from both ends of a string",
                    "parameters": [{"name": "str", "type": "string"}],
                    "return_type": "string",
                    "example": "trim(\"  hello  \") // returns \"hello\""
                }"#
            ),
            (
                "str_repeat",
                r#"{
                    "name": "str_repeat",
                    "description": "Repeats a string a specified number of times",
                    "parameters": [
                        {"name": "str", "type": "string"},
                        {"name": "times", "type": "number"}
                    ],
                    "return_type": "string",
                    "example": "str_repeat(\"ab\", 3) // returns \"ababab\""
                }"#
            ),
            (
                "strpos",
                r#"{
                    "name": "strpos",
                    "description": "Finds the position of the first occurrence of a substring (0-based)",
                    "parameters": [
                        {"name": "haystack", "type": "string"},
                        {"name": "needle", "type": "string"}
                    ],
                    "return_type": "number (position or -1 if not found)",
                    "example": "strpos(\"hello world\", \"world\") // returns 6"
                }"#
            ),
            (
                "strtoupper",
                r#"{
                    "name": "strtoupper",
                    "description": "Converts a string to uppercase",
                    "parameters": [{"name": "str", "type": "string"}],
                    "return_type": "string",
                    "example": "strtoupper(\"hello\") // returns \"HELLO\""
                }"#
            ),
            (
                "strtolower",
                r#"{
                    "name": "strtolower",
                    "description": "Converts a string to lowercase",
                    "parameters": [{"name": "str", "type": "string"}],
                    "return_type": "string",
                    "example": "strtolower(\"HELLO\") // returns \"hello\""
                }"#
            ),
            (
                "str_pad",
                r#"{
                    "name": "str_pad",
                    "description": "Pads a string to a specified length with another string",
                    "parameters": [
                        {"name": "str", "type": "string"},
                        {"name": "length", "type": "number"},
                        {"name": "pad_string", "type": "string"},
                        {"name": "type", "type": "number", "optional": true, "default": "2 (right pad)"}
                    ],
                    "return_type": "string",
                    "example": "str_pad(\"hello\", 10, \" \") // returns \"hello     \""
                }"#
            ),
            (
                "str_reverse",
                r#"{
                    "name": "str_reverse",
                    "description": "Reverses a string",
                    "parameters": [{"name": "str", "type": "string"}],
                    "return_type": "string",
                    "example": "str_reverse(\"hello\") // returns \"olleh\""
                }"#
            ),
            (
                "str_shuffle",
                r#"{
                    "name": "str_shuffle",
                    "description": "Randomly shuffles the characters in a string",
                    "parameters": [{"name": "str", "type": "string"}],
                    "return_type": "string",
                    "example": "str_shuffle(\"abc\") // returns \"bca\" or \"cba\" etc."
                }"#
            ),
            (
                "str_word_count",
                r#"{
                    "name": "str_word_count",
                    "description": "Counts the number of words in a string",
                    "parameters": [{"name": "str", "type": "string"}],
                    "return_type": "number",
                    "example": "str_word_count(\"hello world\") // returns 2"
                }"#
            ),
            (
                "str_ends_with",
                r#"{
                    "name": "str_ends_with",
                    "description": "Checks if a string ends with a given substring",
                    "parameters": [
                        {"name": "haystack", "type": "string"},
                        {"name": "needle", "type": "string"}
                    ],
                    "return_type": "boolean",
                    "example": "str_ends_with(\"hello world\", \"world\") // returns true"
                }"#
            ),
            (
                "str_starts_with",
                r#"{
                    "name": "str_starts_with",
                    "description": "Checks if a string starts with a given substring",
                    "parameters": [
                        {"name": "haystack", "type": "string"},
                        {"name": "needle", "type": "string"}
                    ],
                    "return_type": "boolean",
                    "example": "str_starts_with(\"hello world\", \"hello\") // returns true"
                }"#
            ),
            (
                "str_trim_left",
                r#"{
                    "name": "str_trim_left",
                    "description": "Removes whitespace from the left end of a string",
                    "parameters": [{"name": "str", "type": "string"}],
                    "return_type": "string",
                    "example": "str_trim_left(\"  hello  \") // returns \"hello  \""
                }"#
            ),
            (
                "str_trim_right",
                r#"{
                    "name": "str_trim_right",
                    "description": "Removes whitespace from the right end of a string",
                    "parameters": [{"name": "str", "type": "string"}],
                    "return_type": "string",
                    "example": "str_trim_right(\"  hello  \") // returns \"  hello\""
                }"#
            ),
            (
                "str_swapcase",
                r#"{
                    "name": "str_swapcase",
                    "description": "Swaps the case of each character in a string (upper becomes lower, vice versa)",
                    "parameters": [{"name": "str", "type": "string"}],
                    "return_type": "string",
                    "example": "str_swapcase(\"Hello\") // returns \"hELLO\""
                }"#
            ),
            (
                "str_capitalize",
                r#"{
                    "name": "str_capitalize",
                    "description": "Capitalizes the first character of a string",
                    "parameters": [{"name": "str", "type": "string"}],
                    "return_type": "string",
                    "example": "str_capitalize(\"hello\") // returns \"Hello\""
                }"#
            ),
            (
                "str_title",
                r#"{
                    "name": "str_title",
                    "description": "Converts a string to title case (first letter of each word capitalized)",
                    "parameters": [{"name": "str", "type": "string"}],
                    "return_type": "string",
                    "example": "str_title(\"hello world\") // returns \"Hello World\""
                }"#
            ),
            (
                "str_snake_case",
                r#"{
                    "name": "str_snake_case",
                    "description": "Converts a string to snake_case (lowercase with underscores)",
                    "parameters": [{"name": "str", "type": "string"}],
                    "return_type": "string",
                    "example": "str_snake_case(\"HelloWorld\") // returns \"hello_world\""
                }"#
            ),
            (
                "str_camel_case",
                r#"{
                    "name": "str_camel_case",
                    "description": "Converts a string to camelCase",
                    "parameters": [{"name": "str", "type": "string"}],
                    "return_type": "string",
                    "example": "str_camel_case(\"hello_world\") // returns \"helloWorld\""
                }"#
            ),
            (
                "str_kebab_case",
                r#"{
                    "name": "str_kebab_case",
                    "description": "Converts a string to kebab-case (lowercase with hyphens)",
                    "parameters": [{"name": "str", "type": "string"}],
                    "return_type": "string",
                    "example": "str_kebab_case(\"HelloWorld\") // returns \"hello-world\""
                }"#
            ),
            (
                "substr",
                r#"{
                    "name": "substr",
                    "description": "Extracts a substring from a string",
                    "parameters": [
                        {"name": "str", "type": "string"},
                        {"name": "start", "type": "number"},
                        {"name": "length", "type": "number", "optional": true}
                    ],
                    "return_type": "string",
                    "example": "substr(\"hello world\", 0, 5) // returns \"hello\""
                }"#
            ),
            (
                "str_after",
                r#"{
                    "name": "str_after",
                    "description": "Returns everything after the first occurrence of a substring",
                    "parameters": [
                        {"name": "str", "type": "string"},
                        {"name": "search", "type": "string"}
                    ],
                    "return_type": "string",
                    "example": "str_after(\"hello world\", \" \") // returns \"world\""
                }"#
            ),
            (
                "str_before",
                r#"{
                    "name": "str_before",
                    "description": "Returns everything before the first occurrence of a substring",
                    "parameters": [
                        {"name": "str", "type": "string"},
                        {"name": "search", "type": "string"}
                    ],
                    "return_type": "string",
                    "example": "str_before(\"hello world\", \" \") // returns \"hello\""
                }"#
            ),
            (
                "str_after_last",
                r#"{
                    "name": "str_after_last",
                    "description": "Returns everything after the last occurrence of a substring",
                    "parameters": [
                        {"name": "str", "type": "string"},
                        {"name": "search", "type": "string"}
                    ],
                    "return_type": "string",
                    "example": "str_after_last(\"hello world world\", \" \") // returns \"world\""
                }"#
            ),
            (
                "str_before_last",
                r#"{
                    "name": "str_before_last",
                    "description": "Returns everything before the last occurrence of a substring",
                    "parameters": [
                        {"name": "str", "type": "string"},
                        {"name": "search", "type": "string"}
                    ],
                    "return_type": "string",
                    "example": "str_before_last(\"hello world world\", \" \") // returns \"hello world\""
                }"#
            ),
            (
                "str_is_empty",
                r#"{
                    "name": "str_is_empty",
                    "description": "Checks if a string is empty",
                    "parameters": [{"name": "str", "type": "string"}],
                    "return_type": "boolean",
                    "example": "str_is_empty(\"\") // returns true"
                }"#
            ),
            (
                "str_is_blank",
                r#"{
                    "name": "str_is_blank",
                    "description": "Checks if a string is empty or contains only whitespace",
                    "parameters": [{"name": "str", "type": "string"}],
                    "return_type": "boolean",
                    "example": "str_is_blank(\"   \") // returns true"
                }"#
            ),
            (
                "str_is_numeric",
                r#"{
                    "name": "str_is_numeric",
                    "description": "Checks if a string contains only digits",
                    "parameters": [{"name": "str", "type": "string"}],
                    "return_type": "boolean",
                    "example": "str_is_numeric(\"123\") // returns true"
                }"#
            ),
            (
                "str_is_alpha",
                r#"{
                    "name": "str_is_alpha",
                    "description": "Checks if a string contains only alphabetic characters",
                    "parameters": [{"name": "str", "type": "string"}],
                    "return_type": "boolean",
                    "example": "str_is_alpha(\"abc\") // returns true"
                }"#
            ),
            (
                "str_is_alphanumeric",
                r#"{
                    "name": "str_is_alphanumeric",
                    "description": "Checks if a string contains only alphanumeric characters",
                    "parameters": [{"name": "str", "type": "string"}],
                    "return_type": "boolean",
                    "example": "str_is_alphanumeric(\"abc123\") // returns true"
                }"#
            ),
            (
                "str_is_lowercase",
                r#"{
                    "name": "str_is_lowercase",
                    "description": "Checks if a string is all lowercase",
                    "parameters": [{"name": "str", "type": "string"}],
                    "return_type": "boolean",
                    "example": "str_is_lowercase(\"hello\") // returns true"
                }"#
            ),
            (
                "str_is_uppercase",
                r#"{
                    "name": "str_is_uppercase",
                    "description": "Checks if a string is all uppercase",
                    "parameters": [{"name": "str", "type": "string"}],
                    "return_type": "boolean",
                    "example": "str_is_uppercase(\"HELLO\") // returns true"
                }"#
            ),
            (
                "str_truncate",
                r#"{
                    "name": "str_truncate",
                    "description": "Truncates a string to a specified length and adds '...'",
                    "parameters": [
                        {"name": "str", "type": "string"},
                        {"name": "length", "type": "number"}
                    ],
                    "return_type": "string",
                    "example": "str_truncate(\"hello world\", 5) // returns \"he...\""
                }"#
            ),
            (
                "str_truncate_middle",
                r#"{
                    "name": "str_truncate_middle",
                    "description": "Truncates a string from the middle with '...'",
                    "parameters": [
                        {"name": "str", "type": "string"},
                        {"name": "length", "type": "number"}
                    ],
                    "return_type": "string",
                    "example": "str_truncate_middle(\"hello world\", 7) // returns \"he...ld\""
                }"#
            ),
            (
                "str_reverse_words",
                r#"{
                    "name": "str_reverse_words",
                    "description": "Reverses the order of words in a string",
                    "parameters": [{"name": "str", "type": "string"}],
                    "return_type": "string",
                    "example": "str_reverse_words(\"hello world\") // returns \"world hello\""
                }"#
            ),
            (
                "str_word_wrap",
                r#"{
                    "name": "str_word_wrap",
                    "description": "Wraps a string to a specified width",
                    "parameters": [
                        {"name": "str", "type": "string"},
                        {"name": "width", "type": "number"}
                    ],
                    "return_type": "string",
                    "example": "str_word_wrap(\"hello world\", 5) // returns \"hello\\nworld\""
                }"#
            ),
            (
                "str_remove",
                r#"{
                    "name": "str_remove",
                    "description": "Removes the first occurrence of a substring",
                    "parameters": [
                        {"name": "str", "type": "string"},
                        {"name": "search", "type": "string"}
                    ],
                    "return_type": "string",
                    "example": "str_remove(\"hello world\", \" \") // returns \"helloworld\""
                }"#
            ),
            (
                "str_remove_all",
                r#"{
                    "name": "str_remove_all",
                    "description": "Removes all occurrences of a substring",
                    "parameters": [
                        {"name": "str", "type": "string"},
                        {"name": "search", "type": "string"}
                    ],
                    "return_type": "string",
                    "example": "str_remove_all(\"a b c\", \" \") // returns \"abc\""
                }"#
            ),
            (
                "html_escape",
                r#"{
                    "name": "html_escape",
                    "description": "Escapes HTML special characters (&, <, >, \", ') to HTML entities",
                    "parameters": [{"name": "str", "type": "string"}],
                    "return_type": "string",
                    "example": "html_escape(\"<div>\") // returns \"&lt;div&gt;\""
                }"#
            ),
            (
                "html_unescape",
                r#"{
                    "name": "html_unescape",
                    "description": "Unescapes HTML entities back to normal characters",
                    "parameters": [{"name": "str", "type": "string"}],
                    "return_type": "string",
                    "example": "html_unescape(\"&lt;div&gt;\") // returns \"<div>\""
                }"#
            ),

            (
                "array",
                r#"{
                    "name": "array",
                    "description": "Creates an array from the given arguments",
                    "parameters": [{"name": "values", "type": "any", "variadic": true}],
                    "return_type": "array",
                    "example": "array(1, 2, 3) // returns [1, 2, 3]"
                }"#
            ),
            (
                "array_generator",
                r#"{
                    "name": "array_generator",
                    "description": "Generates an array of numbers from start to end",
                    "parameters": [
                        {"name": "start", "type": "number"},
                        {"name": "end", "type": "number"},
                        {"name": "filter", "type": "function", "optional": true}
                    ],
                    "return_type": "array",
                    "example": "array_generator(1, 5) // returns [1, 2, 3, 4, 5]"
                }"#
            ),
            (
                "range",
                r#"{
                    "name": "range",
                    "description": "Creates an array of numbers from start to end with a step",
                    "parameters": [
                        {"name": "start", "type": "number"},
                        {"name": "end", "type": "number"},
                        {"name": "step", "type": "number", "default": 1}
                    ],
                    "return_type": "array",
                    "example": "range(1, 5, 2) // returns [1, 3, 5]"
                }"#
            ),
            (
                "array_push",
                r#"{
                    "name": "array_push",
                    "description": "Adds one or more elements to the end of an array",
                    "parameters": [
                        {"name": "array", "type": "array"},
                        {"name": "values", "type": "any", "variadic": true}
                    ],
                    "return_type": "number (new length)",
                    "example": "array_push([1, 2], 3, 4) // array becomes [1, 2, 3, 4]"
                }"#
            ),
            (
                "array_pop",
                r#"{
                    "name": "array_pop",
                    "description": "Removes and returns the last element of an array",
                    "parameters": [{"name": "array", "type": "array"}],
                    "return_type": "any (the popped element)",
                    "example": "array_pop([1, 2, 3]) // returns 3, array becomes [1, 2]"
                }"#
            ),
            (
                "array_shift",
                r#"{
                    "name": "array_shift",
                    "description": "Removes and returns the first element of an array",
                    "parameters": [{"name": "array", "type": "array"}],
                    "return_type": "any (the shifted element)",
                    "example": "array_shift([1, 2, 3]) // returns 1, array becomes [2, 3]"
                }"#
            ),
            (
                "array_unshift",
                r#"{
                    "name": "array_unshift",
                    "description": "Adds one or more elements to the beginning of an array",
                    "parameters": [
                        {"name": "array", "type": "array"},
                        {"name": "values", "type": "any", "variadic": true}
                    ],
                    "return_type": "number (new length)",
                    "example": "array_unshift([2, 3], 1) // array becomes [1, 2, 3]"
                }"#
            ),
            (
                "array_len",
                r#"{
                    "name": "array_len",
                    "description": "Returns the length of an array",
                    "parameters": [{"name": "array", "type": "array"}],
                    "return_type": "number",
                    "example": "array_len([1, 2, 3]) // returns 3"
                }"#
            ),
            (
                "array_get",
                r#"{
                    "name": "array_get",
                    "description": "Returns the element at the specified index",
                    "parameters": [
                        {"name": "array", "type": "array"},
                        {"name": "index", "type": "number"}
                    ],
                    "return_type": "any",
                    "example": "array_get([1, 2, 3], 1) // returns 2"
                }"#
            ),
            (
                "array_set",
                r#"{
                    "name": "array_set",
                    "description": "Sets the element at the specified index",
                    "parameters": [
                        {"name": "array", "type": "array"},
                        {"name": "index", "type": "number"},
                        {"name": "value", "type": "any"}
                    ],
                    "return_type": "nil",
                    "example": "array_set([1, 2, 3], 1, 99) // array becomes [1, 99, 3]"
                }"#
            ),
            (
                "array_remove",
                r#"{
                    "name": "array_remove",
                    "description": "Removes and returns the element at a specified index",
                    "parameters": [
                        {"name": "array", "type": "array"},
                        {"name": "index", "type": "number"}
                    ],
                    "return_type": "any (the removed element)",
                    "example": "array_remove([1, 2, 3], 1) // returns 2, array becomes [1, 3]"
                }"#
            ),
            (
                "array_insert",
                r#"{
                    "name": "array_insert",
                    "description": "Inserts an element at a specified index",
                    "parameters": [
                        {"name": "array", "type": "array"},
                        {"name": "index", "type": "number"},
                        {"name": "value", "type": "any"}
                    ],
                    "return_type": "number (new length)",
                    "example": "array_insert([1, 3], 1, 2) // array becomes [1, 2, 3]"
                }"#
            ),
            (
                "array_contains",
                r#"{
                    "name": "array_contains",
                    "description": "Checks if an array contains a value",
                    "parameters": [
                        {"name": "array", "type": "array"},
                        {"name": "value", "type": "any"}
                    ],
                    "return_type": "boolean",
                    "example": "array_contains([1, 2, 3], 2) // returns true"
                }"#
            ),
            (
                "array_index_of",
                r#"{
                    "name": "array_index_of",
                    "description": "Returns the index of the first occurrence of a value",
                    "parameters": [
                        {"name": "array", "type": "array"},
                        {"name": "value", "type": "any"}
                    ],
                    "return_type": "number (index or -1 if not found)",
                    "example": "array_index_of([1, 2, 3], 2) // returns 1"
                }"#
            ),
            (
                "array_last_index_of",
                r#"{
                    "name": "array_last_index_of",
                    "description": "Returns the index of the last occurrence of a value",
                    "parameters": [
                        {"name": "array", "type": "array"},
                        {"name": "value", "type": "any"}
                    ],
                    "return_type": "number (index or -1 if not found)",
                    "example": "array_last_index_of([1, 2, 2, 3], 2) // returns 2"
                }"#
            ),
            (
                "array_min",
                r#"{
                    "name": "array_min",
                    "description": "Returns the minimum value in an array",
                    "parameters": [{"name": "array", "type": "array"}],
                    "return_type": "number",
                    "example": "array_min([3, 1, 2]) // returns 1"
                }"#
            ),
            (
                "array_max",
                r#"{
                    "name": "array_max",
                    "description": "Returns the maximum value in an array",
                    "parameters": [{"name": "array", "type": "array"}],
                    "return_type": "number",
                    "example": "array_max([3, 1, 2]) // returns 3"
                }"#
            ),
            (
                "array_rand",
                r#"{
                    "name": "array_rand",
                    "description": "Returns a random element from an array",
                    "parameters": [{"name": "array", "type": "array"}],
                    "return_type": "any",
                    "example": "array_rand([1, 2, 3]) // returns random element"
                }"#
            ),
            (
                "array_join",
                r#"{
                    "name": "array_join",
                    "description": "Joins array elements into a string with a separator",
                    "parameters": [
                        {"name": "array", "type": "array"},
                        {"name": "separator", "type": "string"}
                    ],
                    "return_type": "string",
                    "example": "array_join([\"a\", \"b\", \"c\"], \", \") // returns \"a, b, c\""
                }"#
            ),
            (
                "array_slice",
                r#"{
                    "name": "array_slice",
                    "description": "Extracts a portion of an array",
                    "parameters": [
                        {"name": "array", "type": "array"},
                        {"name": "start", "type": "number"},
                        {"name": "end", "type": "number"}
                    ],
                    "return_type": "array",
                    "example": "array_slice([1, 2, 3, 4], 1, 3) // returns [2, 3]"
                }"#
            ),
            (
                "array_splice",
                r#"{
                    "name": "array_splice",
                    "description": "Removes a portion of an array and returns it",
                    "parameters": [
                        {"name": "array", "type": "array"},
                        {"name": "start", "type": "number"},
                        {"name": "length", "type": "number"}
                    ],
                    "return_type": "array (removed portion)",
                    "example": "array_splice([1, 2, 3, 4], 1, 2) // returns [2, 3], array becomes [1, 4]"
                }"#
            ),
            (
                "array_reverse",
                r#"{
                    "name": "array_reverse",
                    "description": "Reverses the order of elements in an array",
                    "parameters": [{"name": "array", "type": "array"}],
                    "return_type": "array",
                    "example": "array_reverse([1, 2, 3]) // returns [3, 2, 1]"
                }"#
            ),
            (
                "array_sort",
                r#"{
                    "name": "array_sort",
                    "description": "Sorts an array in ascending order (optionally with a comparator)",
                    "parameters": [
                        {"name": "array", "type": "array"},
                        {"name": "comparator", "type": "function", "optional": true}
                    ],
                    "return_type": "array",
                    "example": "array_sort([3, 1, 2]) // returns [1, 2, 3]"
                }"#
            ),
            (
                "array_rsort",
                r#"{
                    "name": "array_rsort",
                    "description": "Sorts an array in descending order (optionally with a comparator)",
                    "parameters": [
                        {"name": "array", "type": "array"},
                        {"name": "comparator", "type": "function", "optional": true}
                    ],
                    "return_type": "array",
                    "example": "array_rsort([1, 2, 3]) // returns [3, 2, 1]"
                }"#
            ),
            (
                "array_clear",
                r#"{
                    "name": "array_clear",
                    "description": "Removes all elements from an array",
                    "parameters": [{"name": "array", "type": "array"}],
                    "return_type": "nil",
                    "example": "array_clear([1, 2, 3]) // array becomes []"
                }"#
            ),
            (
                "array_copy",
                r#"{
                    "name": "array_copy",
                    "description": "Creates a copy of an array",
                    "parameters": [{"name": "array", "type": "array"}],
                    "return_type": "array",
                    "example": "array_copy([1, 2, 3]) // returns [1, 2, 3]"
                }"#
            ),
            (
                "array_merge",
                r#"{
                    "name": "array_merge",
                    "description": "Merges two arrays into one",
                    "parameters": [
                        {"name": "array1", "type": "array"},
                        {"name": "array2", "type": "array"}
                    ],
                    "return_type": "array",
                    "example": "array_merge([1, 2], [3, 4]) // returns [1, 2, 3, 4]"
                }"#
            ),
            (
                "array_unique",
                r#"{
                    "name": "array_unique",
                    "description": "Removes duplicate values from an array",
                    "parameters": [{"name": "array", "type": "array"}],
                    "return_type": "array",
                    "example": "array_unique([1, 2, 2, 3]) // returns [1, 2, 3]"
                }"#
            ),
            (
                "array_sum",
                r#"{
                    "name": "array_sum",
                    "description": "Returns the sum of numeric values in an array",
                    "parameters": [{"name": "array", "type": "array"}],
                    "return_type": "number",
                    "example": "array_sum([1, 2, 3]) // returns 6"
                }"#
            ),
            (
                "array_map",
                r#"{
                    "name": "array_map",
                    "description": "Applies a function to each element of an array",
                    "parameters": [
                        {"name": "array", "type": "array"},
                        {"name": "callback", "type": "function"}
                    ],
                    "return_type": "array",
                    "example": "array_map([1, 2, 3], lambda fn(x) { return x * 2; }) // returns [2, 4, 6]"
                }"#
            ),
            (
                "array_filter",
                r#"{
                    "name": "array_filter",
                    "description": "Filters array elements using a callback function",
                    "parameters": [
                        {"name": "array", "type": "array"},
                        {"name": "callback", "type": "function"}
                    ],
                    "return_type": "array",
                    "example": "array_filter([1, 2, 3, 4], lambda fn(x) { return x % 2 == 0; }) // returns [2, 4]"
                }"#
            ),
            (
                "array_reduce",
                r#"{
                    "name": "array_reduce",
                    "description": "Reduces an array to a single value using a callback",
                    "parameters": [
                        {"name": "array", "type": "array"},
                        {"name": "callback", "type": "function"},
                        {"name": "initial", "type": "any"}
                    ],
                    "return_type": "any",
                    "example": "array_reduce([1, 2, 3], lambda fn(acc, x) { return acc + x; }, 0) // returns 6"
                }"#
            ),

            (
                "add",
                r#"{
                    "name": "add",
                    "description": "Adds two numbers",
                    "parameters": [
                        {"name": "a", "type": "number"},
                        {"name": "b", "type": "number"}
                    ],
                    "return_type": "number",
                    "example": "add(5, 3) // returns 8"
                }"#
            ),
            (
                "subtract",
                r#"{
                    "name": "subtract",
                    "description": "Subtracts two numbers (a - b)",
                    "parameters": [
                        {"name": "a", "type": "number"},
                        {"name": "b", "type": "number"}
                    ],
                    "return_type": "number",
                    "example": "subtract(10, 3) // returns 7"
                }"#
            ),
            (
                "multiplication",
                r#"{
                    "name": "multiplication",
                    "description": "Multiplies two numbers",
                    "parameters": [
                        {"name": "a", "type": "number"},
                        {"name": "b", "type": "number"}
                    ],
                    "return_type": "number",
                    "example": "multiplication(4, 3) // returns 12"
                }"#
            ),
            (
                "division",
                r#"{
                    "name": "division",
                    "description": "Divides two numbers (a / b)",
                    "parameters": [
                        {"name": "a", "type": "number"},
                        {"name": "b", "type": "number"}
                    ],
                    "return_type": "number",
                    "example": "division(10, 2) // returns 5"
                }"#
            ),
            (
                "sin",
                r#"{
                    "name": "sin",
                    "description": "Returns the sine of an angle (in radians)",
                    "parameters": [{"name": "x", "type": "number"}],
                    "return_type": "number",
                    "example": "sin(0) // returns 0.0"
                }"#
            ),
            (
                "cos",
                r#"{
                    "name": "cos",
                    "description": "Returns the cosine of an angle (in radians)",
                    "parameters": [{"name": "x", "type": "number"}],
                    "return_type": "number",
                    "example": "cos(0) // returns 1.0"
                }"#
            ),
            (
                "tan",
                r#"{
                    "name": "tan",
                    "description": "Returns the tangent of an angle (in radians)",
                    "parameters": [{"name": "x", "type": "number"}],
                    "return_type": "number",
                    "example": "tan(0) // returns 0.0"
                }"#
            ),
            (
                "asin",
                r#"{
                    "name": "asin",
                    "description": "Returns the arc sine of a value (in radians)",
                    "parameters": [{"name": "x", "type": "number"}],
                    "return_type": "number",
                    "example": "asin(0) // returns 0.0"
                }"#
            ),
            (
                "acos",
                r#"{
                    "name": "acos",
                    "description": "Returns the arc cosine of a value (in radians)",
                    "parameters": [{"name": "x", "type": "number"}],
                    "return_type": "number",
                    "example": "acos(1) // returns 0.0"
                }"#
            ),
            (
                "atan",
                r#"{
                    "name": "atan",
                    "description": "Returns the arc tangent of a value (in radians)",
                    "parameters": [{"name": "x", "type": "number"}],
                    "return_type": "number",
                    "example": "atan(1) // returns 0.785398..."
                }"#
            ),
            (
                "atan2",
                r#"{
                    "name": "atan2",
                    "description": "Returns the arc tangent of y/x (in radians)",
                    "parameters": [
                        {"name": "y", "type": "number"},
                        {"name": "x", "type": "number"}
                    ],
                    "return_type": "number",
                    "example": "atan2(1, 1) // returns 0.785398..."
                }"#
            ),
            (
                "sinh",
                r#"{
                    "name": "sinh",
                    "description": "Returns the hyperbolic sine of a number",
                    "parameters": [{"name": "x", "type": "number"}],
                    "return_type": "number",
                    "example": "sinh(0) // returns 0.0"
                }"#
            ),
            (
                "cosh",
                r#"{
                    "name": "cosh",
                    "description": "Returns the hyperbolic cosine of a number",
                    "parameters": [{"name": "x", "type": "number"}],
                    "return_type": "number",
                    "example": "cosh(0) // returns 1.0"
                }"#
            ),
            (
                "tanh",
                r#"{
                    "name": "tanh",
                    "description": "Returns the hyperbolic tangent of a number",
                    "parameters": [{"name": "x", "type": "number"}],
                    "return_type": "number",
                    "example": "tanh(0) // returns 0.0"
                }"#
            ),
            (
                "asinh",
                r#"{
                    "name": "asinh",
                    "description": "Returns the inverse hyperbolic sine of a number",
                    "parameters": [{"name": "x", "type": "number"}],
                    "return_type": "number",
                    "example": "asinh(0) // returns 0.0"
                }"#
            ),
            (
                "acosh",
                r#"{
                    "name": "acosh",
                    "description": "Returns the inverse hyperbolic cosine of a number",
                    "parameters": [{"name": "x", "type": "number"}],
                    "return_type": "number",
                    "example": "acosh(1) // returns 0.0"
                }"#
            ),
            (
                "atanh",
                r#"{
                    "name": "atanh",
                    "description": "Returns the inverse hyperbolic tangent of a number",
                    "parameters": [{"name": "x", "type": "number"}],
                    "return_type": "number",
                    "example": "atanh(0) // returns 0.0"
                }"#
            ),
            (
                "exp",
                r#"{
                    "name": "exp",
                    "description": "Returns e raised to the power of x",
                    "parameters": [{"name": "x", "type": "number"}],
                    "return_type": "number",
                    "example": "exp(1) // returns 2.718281..."
                }"#
            ),
            (
                "log",
                r#"{
                    "name": "log",
                    "description": "Returns the natural logarithm (base e) of a number",
                    "parameters": [{"name": "x", "type": "number"}],
                    "return_type": "number",
                    "example": "log(2.71828) // returns 1.0"
                }"#
            ),
            (
                "log10",
                r#"{
                    "name": "log10",
                    "description": "Returns the base-10 logarithm of a number",
                    "parameters": [{"name": "x", "type": "number"}],
                    "return_type": "number",
                    "example": "log10(100) // returns 2.0"
                }"#
            ),
            (
                "log2",
                r#"{
                    "name": "log2",
                    "description": "Returns the base-2 logarithm of a number",
                    "parameters": [{"name": "x", "type": "number"}],
                    "return_type": "number",
                    "example": "log2(8) // returns 3.0"
                }"#
            ),
            (
                "pow",
                r#"{
                    "name": "pow",
                    "description": "Returns the result of raising a number to a power (base ^ exponent)",
                    "parameters": [
                        {"name": "base", "type": "number"},
                        {"name": "exponent", "type": "number"}
                    ],
                    "return_type": "number",
                    "example": "pow(2, 3) // returns 8.0"
                }"#
            ),
            (
                "sqrt",
                r#"{
                    "name": "sqrt",
                    "description": "Returns the square root of a number",
                    "parameters": [{"name": "x", "type": "number"}],
                    "return_type": "number",
                    "example": "sqrt(9) // returns 3.0"
                }"#
            ),
            (
                "cbrt",
                r#"{
                    "name": "cbrt",
                    "description": "Returns the cube root of a number",
                    "parameters": [{"name": "x", "type": "number"}],
                    "return_type": "number",
                    "example": "cbrt(27) // returns 3.0"
                }"#
            ),
            (
                "hypot",
                r#"{
                    "name": "hypot",
                    "description": "Returns the hypotenuse (sqrt(x² + y²))",
                    "parameters": [
                        {"name": "x", "type": "number"},
                        {"name": "y", "type": "number"}
                    ],
                    "return_type": "number",
                    "example": "hypot(3, 4) // returns 5.0"
                }"#
            ),
            (
                "factorial",
                r#"{
                    "name": "factorial",
                    "description": "Returns the factorial of a non-negative integer (n!)",
                    "parameters": [{"name": "n", "type": "int"}],
                    "return_type": "number",
                    "example": "factorial(5) // returns 120"
                }"#
            ),
            (
                "permutation",
                r#"{
                    "name": "permutation",
                    "description": "Returns the number of permutations (nPr = n! / (n-r)!)",
                    "parameters": [
                        {"name": "n", "type": "int"},
                        {"name": "r", "type": "int"}
                    ],
                    "return_type": "number",
                    "example": "permutation(5, 2) // returns 20"
                }"#
            ),
            (
                "combination",
                r#"{
                    "name": "combination",
                    "description": "Returns the number of combinations (nCr = n! / (r! * (n-r)!))",
                    "parameters": [
                        {"name": "n", "type": "int"},
                        {"name": "r", "type": "int"}
                    ],
                    "return_type": "number",
                    "example": "combination(5, 2) // returns 10"
                }"#
            ),
            (
                "gcd",
                r#"{
                    "name": "gcd",
                    "description": "Returns the greatest common divisor of two integers",
                    "parameters": [
                        {"name": "a", "type": "int"},
                        {"name": "b", "type": "int"}
                    ],
                    "return_type": "int",
                    "example": "gcd(48, 18) // returns 6"
                }"#
            ),
            (
                "lcm",
                r#"{
                    "name": "lcm",
                    "description": "Returns the least common multiple of two integers",
                    "parameters": [
                        {"name": "a", "type": "int"},
                        {"name": "b", "type": "int"}
                    ],
                    "return_type": "int",
                    "example": "lcm(4, 6) // returns 12"
                }"#
            ),
            (
                "abs",
                r#"{
                    "name": "abs",
                    "description": "Returns the absolute value of a number",
                    "parameters": [{"name": "x", "type": "number"}],
                    "return_type": "number",
                    "example": "abs(-5) // returns 5.0"
                }"#
            ),
            (
                "ceil",
                r#"{
                    "name": "ceil",
                    "description": "Rounds a number up to the nearest integer",
                    "parameters": [{"name": "x", "type": "number"}],
                    "return_type": "number",
                    "example": "ceil(2.3) // returns 3.0"
                }"#
            ),
            (
                "floor",
                r#"{
                    "name": "floor",
                    "description": "Rounds a number down to the nearest integer",
                    "parameters": [{"name": "x", "type": "number"}],
                    "return_type": "number",
                    "example": "floor(2.9) // returns 2.0"
                }"#
            ),
            (
                "round",
                r#"{
                    "name": "round",
                    "description": "Rounds a number to the nearest integer",
                    "parameters": [{"name": "x", "type": "number"}],
                    "return_type": "number",
                    "example": "round(2.5) // returns 3.0"
                }"#
            ),
            (
                "trunc",
                r#"{
                    "name": "trunc",
                    "description": "Truncates a number towards zero (removes decimal part)",
                    "parameters": [{"name": "x", "type": "number"}],
                    "return_type": "number",
                    "example": "trunc(2.9) // returns 2.0"
                }"#
            ),
            (
                "min",
                r#"{
                    "name": "min",
                    "description": "Returns the minimum of the given numbers",
                    "parameters": [{"name": "numbers", "type": "number", "variadic": true}],
                    "return_type": "number",
                    "example": "min(5, 3, 8) // returns 3"
                }"#
            ),
            (
                "max",
                r#"{
                    "name": "max",
                    "description": "Returns the maximum of the given numbers",
                    "parameters": [{"name": "numbers", "type": "number", "variadic": true}],
                    "return_type": "number",
                    "example": "max(5, 3, 8) // returns 8"
                }"#
            ),
            (
                "rand",
                r#"{
                    "name": "rand",
                    "description": "Generates a random integer between min and max (inclusive)",
                    "parameters": [
                        {"name": "min", "type": "number"},
                        {"name": "max", "type": "number"}
                    ],
                    "return_type": "number",
                    "example": "rand(1, 10) // returns random number between 1 and 10"
                }"#
            ),
            (
                "rand_float",
                r#"{
                    "name": "rand_float",
                    "description": "Generates a random float between 0.0 and 1.0",
                    "parameters": [],
                    "return_type": "number",
                    "example": "rand_float() // returns random number between 0.0 and 1.0"
                }"#
            ),
            (
                "pi",
                r#"{
                    "name": "pi",
                    "description": "Returns the mathematical constant π (pi)",
                    "parameters": [],
                    "return_type": "number",
                    "example": "pi() // returns 3.141592653589793"
                }"#
            ),
            (
                "e",
                r#"{
                    "name": "e",
                    "description": "Returns the mathematical constant e (Euler's number)",
                    "parameters": [],
                    "return_type": "number",
                    "example": "e() // returns 2.718281828459045"
                }"#
            ),

            (
                "time",
                r#"{
                    "name": "time",
                    "description": "Returns the current Unix timestamp (seconds since 1970-01-01)",
                    "parameters": [],
                    "return_type": "number",
                    "example": "time() // returns current timestamp"
                }"#
            ),
            (
                "date",
                r#"{
                    "name": "date",
                    "description": "Returns a formatted date string based on the current time",
                    "parameters": [{"name": "format", "type": "string"}],
                    "return_type": "string",
                    "example": "date(\"Y-m-d H:i:s\") // returns \"2024-01-01 12:00:00\""
                }"#
            ),
            (
                "sleep",
                r#"{
                    "name": "sleep",
                    "description": "Pauses execution for a specified number of seconds",
                    "parameters": [{"name": "seconds", "type": "number"}],
                    "return_type": "nil",
                    "example": "sleep(1) // waits 1 second"
                }"#
            ),

            (
                "getenv",
                r#"{
                    "name": "getenv",
                    "description": "Returns the value of an environment variable",
                    "parameters": [{"name": "name", "type": "string"}],
                    "return_type": "string or nil if not found",
                    "example": "getenv(\"HOME\") // returns home directory"
                }"#
            ),
            (
                "setenv",
                r#"{
                    "name": "setenv",
                    "description": "Sets an environment variable",
                    "parameters": [
                        {"name": "name", "type": "string"},
                        {"name": "value", "type": "string"}
                    ],
                    "return_type": "nil",
                    "example": "setenv(\"MY_VAR\", \"hello\")"
                }"#
            ),
            (
                "exit",
                r#"{
                    "name": "exit",
                    "description": "Exits the program with a status code",
                    "parameters": [{"name": "status", "type": "number"}],
                    "return_type": "never",
                    "example": "exit(0) // exits successfully"
                }"#
            ),
            (
                "system",
                r#"{
                    "name": "system",
                    "description": "Executes a system command and returns the exit code",
                    "parameters": [{"name": "command", "type": "string"}],
                    "return_type": "number (exit code)",
                    "example": "system(\"ls -la\")"
                }"#
            ),

            (
                "global",
                r#"{
                    "name": "global",
                    "description": "Gets a variable from the global scope",
                    "parameters": [{"name": "name", "type": "string"}],
                    "return_type": "any",
                    "example": "global(\"MY_GLOBAL\")"
                }"#
            ),
            (
                "global_set",
                r#"{
                    "name": "global_set",
                    "description": "Sets a variable in the global scope",
                    "parameters": [
                        {"name": "name", "type": "string"},
                        {"name": "value", "type": "any"}
                    ],
                    "return_type": "nil",
                    "example": "global_set(\"MY_GLOBAL\", 123)"
                }"#
            ),
            (
                "parent",
                r#"{
                    "name": "parent",
                    "description": "Gets a variable from the parent scope",
                    "parameters": [{"name": "name", "type": "string"}],
                    "return_type": "any",
                    "example": "parent(\"x\")"
                }"#
            ),
            (
                "parent_set",
                r#"{
                    "name": "parent_set",
                    "description": "Sets a variable in the parent scope",
                    "parameters": [
                        {"name": "name", "type": "string"},
                        {"name": "value", "type": "any"}
                    ],
                    "return_type": "nil",
                    "example": "parent_set(\"x\", 123)"
                }"#
            ),

            (
                "fopen",
                r#"{
                    "name": "fopen",
                    "description": "Opens a file and returns a file handle",
                    "parameters": [
                        {"name": "filename", "type": "string"},
                        {"name": "mode", "type": "string (r, w, a, r+, w+, a+)"}
                    ],
                    "return_type": "file_handle or nil on error",
                    "example": "$f = fopen(\"data.txt\", \"r\")"
                }"#
            ),
            (
                "fclose",
                r#"{
                    "name": "fclose",
                    "description": "Closes an open file handle",
                    "parameters": [{"name": "handle", "type": "file_handle"}],
                    "return_type": "nil",
                    "example": "fclose(file)"
                }"#
            ),
            (
                "fwrite",
                r#"{
                    "name": "fwrite",
                    "description": "Writes a string to an open file",
                    "parameters": [
                        {"name": "handle", "type": "file_handle"},
                        {"name": "data", "type": "string"}
                    ],
                    "return_type": "number (bytes written)",
                    "example": "fwrite(file, \"hello\")"
                }"#
            ),
            (
                "fwrite_line",
                r#"{
                    "name": "fwrite_line",
                    "description": "Writes a string to an open file with a newline",
                    "parameters": [
                        {"name": "handle", "type": "file_handle"},
                        {"name": "data", "type": "string"}
                    ],
                    "return_type": "number (bytes written)",
                    "example": "fwrite_line(file, \"hello\")"
                }"#
            ),
            (
                "fread",
                r#"{
                    "name": "fread",
                    "description": "Reads the entire contents of an open file",
                    "parameters": [{"name": "handle", "type": "file_handle"}],
                    "return_type": "string",
                    "example": "$content = fread(file)"
                }"#
            ),
            (
                "fread_line",
                r#"{
                    "name": "fread_line",
                    "description": "Reads one line from an open file",
                    "parameters": [{"name": "handle", "type": "file_handle"}],
                    "return_type": "string or nil if EOF",
                    "example": "$line = fread_line(file)"
                }"#
            ),
            (
                "fread_lines",
                r#"{
                    "name": "fread_lines",
                    "description": "Reads all lines from an open file into an array",
                    "parameters": [{"name": "handle", "type": "file_handle"}],
                    "return_type": "array of strings",
                    "example": "$lines = fread_lines(file)"
                }"#
            ),
            (
                "fseek",
                r#"{
                    "name": "fseek",
                    "description": "Sets the file position indicator",
                    "parameters": [
                        {"name": "handle", "type": "file_handle"},
                        {"name": "offset", "type": "number"},
                        {"name": "whence", "type": "number (0=SET, 1=CUR, 2=END)"}
                    ],
                    "return_type": "number (0 on success, -1 on error)",
                    "example": "fseek(file, 0, 0) // seeks to beginning"
                }"#
            ),
            (
                "ftell",
                r#"{
                    "name": "ftell",
                    "description": "Returns the current file position",
                    "parameters": [{"name": "handle", "type": "file_handle"}],
                    "return_type": "number (position)",
                    "example": "$pos = ftell(file)"
                }"#
            ),
            (
                "feof",
                r#"{
                    "name": "feof",
                    "description": "Checks if the end of file has been reached",
                    "parameters": [{"name": "handle", "type": "file_handle"}],
                    "return_type": "boolean",
                    "example": "feof(file) // returns true if at EOF"
                }"#
            ),
            (
                "rewind",
                r#"{
                    "name": "rewind",
                    "description": "Moves the file position to the beginning",
                    "parameters": [{"name": "handle", "type": "file_handle"}],
                    "return_type": "nil",
                    "example": "rewind(file)"
                }"#
            ),
            (
                "fflush",
                r#"{
                    "name": "fflush",
                    "description": "Flushes any buffered output to a file",
                    "parameters": [{"name": "handle", "type": "file_handle"}],
                    "return_type": "nil",
                    "example": "fflush(file)"
                }"#
            ),
            (
                "file_exists",
                r#"{
                    "name": "file_exists",
                    "description": "Checks if a file exists",
                    "parameters": [{"name": "filename", "type": "string"}],
                    "return_type": "boolean",
                    "example": "file_exists(\"data.txt\") // returns true or false"
                }"#
            ),
            (
                "file_remove",
                r#"{
                    "name": "file_remove",
                    "description": "Deletes a file",
                    "parameters": [{"name": "filename", "type": "string"}],
                    "return_type": "boolean (success)",
                    "example": "file_remove(\"data.txt\")"
                }"#
            ),
            (
                "file_rename",
                r#"{
                    "name": "file_rename",
                    "description": "Renames a file or directory",
                    "parameters": [
                        {"name": "oldname", "type": "string"},
                        {"name": "newname", "type": "string"}
                    ],
                    "return_type": "boolean (success)",
                    "example": "file_rename(\"old.txt\", \"new.txt\")"
                }"#
            ),
            (
                "file_copy",
                r#"{
                    "name": "file_copy",
                    "description": "Copies a file",
                    "parameters": [
                        {"name": "source", "type": "string"},
                        {"name": "destination", "type": "string"}
                    ],
                    "return_type": "boolean (success)",
                    "example": "file_copy(\"src.txt\", \"dest.txt\")"
                }"#
            ),
            (
                "file_move",
                r#"{
                    "name": "file_move",
                    "description": "Moves (renames) a file",
                    "parameters": [
                        {"name": "source", "type": "string"},
                        {"name": "destination", "type": "string"}
                    ],
                    "return_type": "boolean (success)",
                    "example": "file_move(\"src.txt\", \"dest.txt\")"
                }"#
            ),
            (
                "file_size",
                r#"{
                    "name": "file_size",
                    "description": "Returns the size of a file in bytes",
                    "parameters": [{"name": "filename", "type": "string"}],
                    "return_type": "number (size in bytes)",
                    "example": "file_size(\"data.txt\")"
                }"#
            ),
            (
                "file_modified",
                r#"{
                    "name": "file_modified",
                    "description": "Returns the last modification time of a file as a timestamp",
                    "parameters": [{"name": "filename", "type": "string"}],
                    "return_type": "number (timestamp) or nil on error",
                    "example": "file_modified(\"data.txt\")"
                }"#
            ),
            (
                "file_created",
                r#"{
                    "name": "file_created",
                    "description": "Returns the creation time of a file as a timestamp",
                    "parameters": [{"name": "filename", "type": "string"}],
                    "return_type": "number (timestamp) or nil on error",
                    "example": "file_created(\"data.txt\")"
                }"#
            ),
            (
                "file_is_dir",
                r#"{
                    "name": "file_is_dir",
                    "description": "Checks if a path is a directory",
                    "parameters": [{"name": "path", "type": "string"}],
                    "return_type": "boolean",
                    "example": "file_is_dir(\"/home\") // returns true"
                }"#
            ),
            (
                "file_is_file",
                r#"{
                    "name": "file_is_file",
                    "description": "Checks if a path is a regular file",
                    "parameters": [{"name": "path", "type": "string"}],
                    "return_type": "boolean",
                    "example": "file_is_file(\"data.txt\") // returns true"
                }"#
            ),
            (
                "file_append",
                r#"{
                    "name": "file_append",
                    "description": "Appends content to a file",
                    "parameters": [
                        {"name": "filename", "type": "string"},
                        {"name": "content", "type": "string"}
                    ],
                    "return_type": "number (bytes written)",
                    "example": "file_append(\"log.txt\", \"new line\\n\")"
                }"#
            ),
            (
                "file_write_lines",
                r#"{
                    "name": "file_write_lines",
                    "description": "Writes an array of lines to a file",
                    "parameters": [
                        {"name": "filename", "type": "string"},
                        {"name": "lines", "type": "array"}
                    ],
                    "return_type": "number (lines written)",
                    "example": "file_write_lines(\"data.txt\", [\"line1\", \"line2\"])"
                }"#
            ),
            (
                "mkdir",
                r#"{
                    "name": "mkdir",
                    "description": "Creates a directory (creates parent directories if needed)",
                    "parameters": [{"name": "path", "type": "string"}],
                    "return_type": "boolean (success)",
                    "example": "mkdir(\"new_folder/subfolder\")"
                }"#
            ),
            (
                "rmdir",
                r#"{
                    "name": "rmdir",
                    "description": "Removes an empty directory",
                    "parameters": [{"name": "path", "type": "string"}],
                    "return_type": "boolean (success)",
                    "example": "rmdir(\"empty_folder\")"
                }"#
            ),
            (
                "read_dir",
                r#"{
                    "name": "read_dir",
                    "description": "Lists files and directories in a folder",
                    "parameters": [{"name": "path", "type": "string"}],
                    "return_type": "array of strings (filenames)",
                    "example": "read_dir(\".\") // returns [\"file1.prism\", \"file2.prism\"]"
                }"#
            ),

            (
                "is_enum",
                r#"{
                    "name": "is_enum",
                    "description": "Checks if a value is an enum",
                    "parameters": [{"name": "value", "type": "any"}],
                    "return_type": "boolean",
                    "example": "is_enum(MyEnum::Value) // returns true"
                }"#
            ),
            (
                "enum_name",
                r#"{
                    "name": "enum_name",
                    "description": "Returns the name of an enum",
                    "parameters": [{"name": "enum_value", "type": "enum"}],
                    "return_type": "string",
                    "example": "enum_name(MyEnum::Value) // returns \"MyEnum\""
                }"#
            ),
            (
                "enum_variant",
                r#"{
                    "name": "enum_variant",
                    "description": "Returns the variant name of an enum",
                    "parameters": [{"name": "enum_value", "type": "enum"}],
                    "return_type": "string",
                    "example": "enum_variant(MyEnum::Value) // returns \"Value\""
                }"#
            ),

            (
                "map_get_value",
                r#"{
                    "name": "map_get_value",
                    "description": "Gets a value from a map by key",
                    "parameters": [
                        {"name": "map", "type": "map"},
                        {"name": "key", "type": "any"}
                    ],
                    "return_type": "any or nil if key not found",
                    "example": "map_get_value(my_map, \"name\")"
                }"#
            ),
            (
                "map_push",
                r#"{
                    "name": "map_push",
                    "description": "Inserts a key-value pair into a map",
                    "parameters": [
                        {"name": "map", "type": "map"},
                        {"name": "key", "type": "any"},
                        {"name": "value", "type": "any"}
                    ],
                    "return_type": "nil",
                    "example": "map_push(my_map, \"name\", \"John\")"
                }"#
            ),
            (
                "map_peek",
                r#"{
                    "name": "map_peek",
                    "description": "Returns the first key-value pair from a map",
                    "parameters": [{"name": "map", "type": "map"}],
                    "return_type": "array [key, value] or nil if empty",
                    "example": "map_peek(my_map) // returns [\"key\", \"value\"]"
                }"#
            ),
            (
                "map_get_index",
                r#"{
                    "name": "map_get_index",
                    "description": "Returns the index of a key in a map",
                    "parameters": [
                        {"name": "map", "type": "map"},
                        {"name": "key", "type": "any"}
                    ],
                    "return_type": "number (index or -1 if not found)",
                    "example": "map_get_index(my_map, \"name\") // returns 0"
                }"#
            ),
            (
                "map_remove",
                r#"{
                    "name": "map_remove",
                    "description": "Removes a key-value pair from a map",
                    "parameters": [
                        {"name": "map", "type": "map"},
                        {"name": "key", "type": "any"}
                    ],
                    "return_type": "nil",
                    "example": "map_remove(my_map, \"name\")"
                }"#
            ),
            (
                "map_sort_as_key",
                r#"{
                    "name": "map_sort_as_key",
                    "description": "Returns a sorted array of key-value pairs sorted by key",
                    "parameters": [{"name": "map", "type": "map"}],
                    "return_type": "array of [key, value] pairs",
                    "example": "map_sort_as_key(my_map)"
                }"#
            ),
            (
                "map_sort_as_value",
                r#"{
                    "name": "map_sort_as_value",
                    "description": "Returns a sorted array of key-value pairs sorted by value",
                    "parameters": [{"name": "map", "type": "map"}],
                    "return_type": "array of [key, value] pairs",
                    "example": "map_sort_as_value(my_map)"
                }"#
            ),
            (
                "map_len",
                r#"{
                    "name": "map_len",
                    "description": "Returns the number of entries in a map",
                    "parameters": [{"name": "map", "type": "map"}],
                    "return_type": "number",
                    "example": "map_len(my_map)"
                }"#
            ),
            (
                "map_is_key_exists",
                r#"{
                    "name": "map_is_key_exists",
                    "description": "Checks if a key exists in a map",
                    "parameters": [
                        {"name": "map", "type": "map"},
                        {"name": "key", "type": "any"}
                    ],
                    "return_type": "boolean",
                    "example": "map_is_key_exists(my_map, \"name\")"
                }"#
            ),
            (
                "map_is_value_exists",
                r#"{
                    "name": "map_is_value_exists",
                    "description": "Checks if a value exists in a map",
                    "parameters": [
                        {"name": "map", "type": "map"},
                        {"name": "value", "type": "any"}
                    ],
                    "return_type": "boolean",
                    "example": "map_is_value_exists(my_map, \"John\")"
                }"#
            ),
            (
                "map_get_key",
                r#"{
                    "name": "map_get_key",
                    "description": "Finds the first key associated with a value",
                    "parameters": [
                        {"name": "map", "type": "map"},
                        {"name": "value", "type": "any"}
                    ],
                    "return_type": "any or nil if not found",
                    "example": "map_get_key(my_map, \"John\") // returns \"name\""
                }"#
            ),
            (
                "map_keys",
                r#"{
                    "name": "map_keys",
                    "description": "Returns an array of all keys in a map",
                    "parameters": [{"name": "map", "type": "map"}],
                    "return_type": "array",
                    "example": "map_keys(my_map)"
                }"#
            ),
            (
                "map_values",
                r#"{
                    "name": "map_values",
                    "description": "Returns an array of all values in a map",
                    "parameters": [{"name": "map", "type": "map"}],
                    "return_type": "array",
                    "example": "map_values(my_map)"
                }"#
            ),
            (
                "map_clear",
                r#"{
                    "name": "map_clear",
                    "description": "Removes all entries from a map",
                    "parameters": [{"name": "map", "type": "map"}],
                    "return_type": "nil",
                    "example": "map_clear(my_map)"
                }"#
            ),
            (
                "map_copy",
                r#"{
                    "name": "map_copy",
                    "description": "Creates a copy of a map",
                    "parameters": [{"name": "map", "type": "map"}],
                    "return_type": "map",
                    "example": "map_copy(my_map)"
                }"#
            ),
            (
                "map_merge",
                r#"{
                    "name": "map_merge",
                    "description": "Merges two maps (second map overwrites first if keys conflict)",
                    "parameters": [
                        {"name": "map1", "type": "map"},
                        {"name": "map2", "type": "map"}
                    ],
                    "return_type": "map",
                    "example": "map_merge(map1, map2)"
                }"#
            ),

            (
                "json_encode",
                r#"{
                    "name": "json_encode",
                    "description": "Converts a Prism value to a JSON string",
                    "parameters": [{"name": "value", "type": "any"}],
                    "return_type": "string",
                    "example": "json_encode([1, 2, 3]) // returns \"[1,2,3]\""
                }"#
            ),
            (
                "json_decode",
                r#"{
                    "name": "json_decode",
                    "description": "Parses a JSON string into a Prism value",
                    "parameters": [{"name": "json_str", "type": "string"}],
                    "return_type": "any (array, map, string, number, boolean, nil)",
                    "example": "json_decode(\"[1,2,3]\") // returns [1, 2, 3]"
                }"#
            ),
            (
                "csv_to_json",
                r#"{
                    "name": "csv_to_json",
                    "description": "Converts a CSV file to a JSON string",
                    "parameters": [{"name": "file_path", "type": "string"}],
                    "return_type": "string (JSON)",
                    "example": "csv_to_json(\"data.csv\") // returns JSON string"
                }"#
            ),

            (
                "csv_write",
                r#"{
                    "name": "csv_write",
                    "description": "Writes a 2D array to a CSV file",
                    "parameters": [
                        {"name": "filename", "type": "string"},
                        {"name": "data", "type": "array (2D)"}
                    ],
                    "return_type": "boolean (success)",
                    "example": "csv_write(\"data.csv\", [[\"name\", \"age\"], [\"John\", 30]])"
                }"#
            ),
            (
                "csv_read",
                r#"{
                    "name": "csv_read",
                    "description": "Reads a CSV file into a 2D array",
                    "parameters": [{"name": "filename", "type": "string"}],
                    "return_type": "array (2D)",
                    "example": "csv_read(\"data.csv\") // returns [[\"name\", \"age\"], [\"John\", 30]]"
                }"#
            ),

            (
                "http_get",
                r#"{
                    "name": "http_get",
                    "description": "Performs an HTTP GET request",
                    "parameters": [{"name": "url", "type": "string"}],
                    "return_type": "string (response body)",
                    "example": "http_get(\"https://api.example.com/data\")"
                }"#
            ),
            (
                "http_post",
                r#"{
                    "name": "http_post",
                    "description": "Performs an HTTP POST request",
                    "parameters": [
                        {"name": "url", "type": "string"},
                        {"name": "data", "type": "string"}
                    ],
                    "return_type": "string (response body)",
                    "example": "http_post(\"https://api.example.com\", \"key=value\")"
                }"#
            ),
            (
                "http_request",
                r#"{
                    "name": "http_request",
                    "description": "Performs a custom HTTP request (GET, POST, PUT, DELETE)",
                    "parameters": [
                        {"name": "method", "type": "string"},
                        {"name": "url", "type": "string"},
                        {"name": "headers", "type": "string", "optional": true},
                        {"name": "body", "type": "string", "optional": true}
                    ],
                    "return_type": "string (response body)",
                    "example": "http_request(\"DELETE\", \"https://api.example.com/1\")"
                }"#
            ),

            (
                "get",
                r#"{
                    "name": "get",
                    "description": "Gets a GET parameter from the current HTTP request (server mode)",
                    "parameters": [{"name": "key", "type": "string"}],
                    "return_type": "string or empty string if not found",
                    "example": "get(\"name\") // returns \"John\" if ?name=John"
                }"#
            ),
            (
                "post",
                r#"{
                    "name": "post",
                    "description": "Gets a POST parameter from the current HTTP request (server mode)",
                    "parameters": [{"name": "key", "type": "string"}],
                    "return_type": "string or empty string if not found",
                    "example": "post(\"email\")"
                }"#
            ),
            (
                "request_method",
                r#"{
                    "name": "request_method",
                    "description": "Returns the HTTP method of the current request (server mode)",
                    "parameters": [],
                    "return_type": "string (GET, POST, PUT, DELETE, etc.)",
                    "example": "request_method() // returns \"GET\""
                }"#
            ),
            (
                "terminal",
                r#"{
                    "name": "terminal",
                    "description": "Returns the command-line arguments passed to the script",
                    "parameters": [],
                    "return_type": "array of strings",
                    "example": "terminal() // returns [\"arg1\", \"arg2\"]"
                }"#
            ),
            (
                "var_dump",
                r#"{
                    "name": "var_dump",
                    "description": "Returns all variables in the current scope as a map",
                    "parameters": [],
                    "return_type": "map (variable name -> value)",
                    "example": "var_dump() // returns {\"$x\": 123, \"$y\": \"hello\"}"
                }"#
            ),
            (
                "eval",
                r#"{
                    "name": "eval",
                    "description": "Evaluates a string as Prism code",
                    "parameters": [{"name": "code", "type": "string"}],
                    "return_type": "any (result of the evaluated code)",
                    "example": "eval(\"3 + 4\") // returns 7"
                }"#
            ),

            (
                "sha256_string",
                r#"{
                    "name": "sha256_string",
                    "description": "Calculates the SHA256 hash of a string (hex format)",
                    "parameters": [{"name": "str", "type": "string"}],
                    "return_type": "string (64-character hex hash)",
                    "example": "sha256_string(\"hello\") // returns SHA256 hash"
                }"#
            ),
            (
                "sha256_file",
                r#"{
                    "name": "sha256_file",
                    "description": "Calculates the SHA256 hash of a file (hex format)",
                    "parameters": [{"name": "filename", "type": "string"}],
                    "return_type": "string (64-character hex hash)",
                    "example": "sha256_file(\"data.txt\")"
                }"#
            ),
            (
                "sha512_string",
                r#"{
                    "name": "sha512_string",
                    "description": "Calculates the SHA512 hash of a string (hex format)",
                    "parameters": [{"name": "str", "type": "string"}],
                    "return_type": "string (128-character hex hash)",
                    "example": "sha512_string(\"hello\")"
                }"#
            ),
            (
                "sha512_file",
                r#"{
                    "name": "sha512_file",
                    "description": "Calculates the SHA512 hash of a file (hex format)",
                    "parameters": [{"name": "filename", "type": "string"}],
                    "return_type": "string (128-character hex hash)",
                    "example": "sha512_file(\"data.txt\")"
                }"#
            ),
            (
                "uuid_v4",
                r#"{
                    "name": "uuid_v4",
                    "description": "Generates a random UUID v4 string",
                    "parameters": [],
                    "return_type": "string (UUID format: xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx)",
                    "example": "uuid_v4() // returns \"a1b2c3d4-e5f6-7890-abcd-ef1234567890\""
                }"#
            ),

            (
                "base64_encode",
                r#"{
                    "name": "base64_encode",
                    "description": "Encodes a string to Base64",
                    "parameters": [{"name": "str", "type": "string"}],
                    "return_type": "string (Base64 encoded)",
                    "example": "base64_encode(\"hello\") // returns \"aGVsbG8=\""
                }"#
            ),
            (
                "base64_decode",
                r#"{
                    "name": "base64_decode",
                    "description": "Decodes a Base64 string back to original",
                    "parameters": [{"name": "str", "type": "string"}],
                    "return_type": "string (decoded)",
                    "example": "base64_decode(\"aGVsbG8=\") // returns \"hello\""
                }"#
            ),

            (
                "regex_test",
                r#"{
                    "name": "regex_test",
                    "description": "Tests if a regex pattern matches a string",
                    "parameters": [
                        {"name": "pattern", "type": "string"},
                        {"name": "text", "type": "string"},
                        {"name": "flags", "type": "string", "optional": true}
                    ],
                    "return_type": "boolean",
                    "example": "regex_test(\"^[0-9]+$\", \"123\") // returns true"
                }"#
            ),
            (
                "regex_find",
                r#"{
                    "name": "regex_find",
                    "description": "Finds the first match of a regex pattern",
                    "parameters": [
                        {"name": "pattern", "type": "string"},
                        {"name": "text", "type": "string"},
                        {"name": "group", "type": "number", "optional": true, "default": 0},
                        {"name": "flags", "type": "string", "optional": true}
                    ],
                    "return_type": "string or nil if no match",
                    "example": "regex_find(\"\\d+\", \"abc123def\") // returns \"123\""
                }"#
            ),
            (
                "regex_replace",
                r#"{
                    "name": "regex_replace",
                    "description": "Replaces matches of a regex pattern with a replacement",
                    "parameters": [
                        {"name": "pattern", "type": "string"},
                        {"name": "text", "type": "string"},
                        {"name": "replacement", "type": "string"},
                        {"name": "flags", "type": "string", "optional": true}
                    ],
                    "return_type": "string",
                    "example": "regex_replace(\"\\d+\", \"abc123def\", \"X\") // returns \"abcXdef\""
                }"#
            ),
            (
                "regex_split",
                r#"{
                    "name": "regex_split",
                    "description": "Splits a string by regex pattern",
                    "parameters": [
                        {"name": "pattern", "type": "string"},
                        {"name": "text", "type": "string"},
                        {"name": "flags", "type": "string", "optional": true}
                    ],
                    "return_type": "array of strings",
                    "example": "regex_split(\",\", \"a,b,c\") // returns [\"a\", \"b\", \"c\"]"
                }"#
            ),
            (
                "regex_escape",
                r#"{
                    "name": "regex_escape",
                    "description": "Escapes special regex characters in a string",
                    "parameters": [{"name": "str", "type": "string"}],
                    "return_type": "string (escaped)",
                    "example": "regex_escape(\"a.b\") // returns \"a\\.b\""
                }"#
            ),
            (
                "db_open",
                r#"{
                    "name": "db_open",
                    "description": "Opens or creates a SQLite database file and returns a database handle",
                    "parameters": [{"name": "filename", "type": "string"}],
                    "return_type": "database handle",
                    "example": "$db = db_open(\"mydata.db\")"
                }"#
            ),
            (
                "db_close",
                r#"{
                    "name": "db_close",
                    "description": "Closes an open database connection and frees resources",
                    "parameters": [{"name": "db", "type": "database handle"}],
                    "return_type": "nil",
                    "example": "db_close(db)"
                }"#
            ),
            (
                "db_execute",
                r#"{
                    "name": "db_execute",
                    "description": "Executes raw SQL without parameters. Use ONLY for trusted operations like CREATE TABLE, DROP TABLE, or migrations. NEVER pass user input to this function - use db_execute_params or db_query with prepared statements instead.",
                    "parameters": [
                        {"name": "db", "type": "database handle"},
                        {"name": "sql", "type": "string"}
                    ],
                    "return_type": "nil (throws error on failure)",
                    "example": "db_execute(db, \"CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)\")"
                }"#
            ),
            (
                "db_query",
                r#"{
                    "name": "db_query",
                    "description": "Executes a SELECT query with prepared statement parameters. Returns results as a JSON string. Safe from SQL injection because parameters are bound, not concatenated.",
                    "parameters": [
                        {"name": "db", "type": "database handle"},
                        {"name": "sql", "type": "string (use ? for placeholders)"},
                        {"name": "params", "type": "array of values to bind"}
                    ],
                    "return_type": "string (JSON array of row objects)",
                    "example": "$result = db_query(db, \"SELECT * FROM users WHERE name = ?\", [\"Alice\"])"
                }"#
            ),
            (
                "db_execute_params",
                r#"{
                    "name": "db_execute_params",
                    "description": "Executes INSERT, UPDATE, or DELETE with prepared statement parameters. Returns the number of affected rows. Safe from SQL injection because parameters are bound, not concatenated.",
                    "parameters": [
                        {"name": "db", "type": "database handle"},
                        {"name": "sql", "type": "string (use ? for placeholders)"},
                        {"name": "params", "type": "array of values to bind"}
                    ],
                    "return_type": "number (rows affected, or -1 on error)",
                    "example": "$affected = db_execute_params(db, \"INSERT INTO users (name) VALUES (?)\", [\"Alice\"])"
                }"#
            ),
            (
                "db_last_insert_id",
                r#"{
                    "name": "db_last_insert_id",
                    "description": "Returns the row ID of the most recent INSERT on this database connection",
                    "parameters": [{"name": "db", "type": "database handle"}],
                    "return_type": "number (row ID)",
                    "example": "$new_id = db_last_insert_id(db)"
                }"#
            ),
            (
                "db_error",
                r#"{
                    "name": "db_error",
                    "description": "Returns the last error message from the database connection",
                    "parameters": [{"name": "db", "type": "database handle"}],
                    "return_type": "string",
                    "example": "$err = db_error(db)"
                }"#
            ),
            (
                "chr",
                r#"{
                    "name": "chr",
                    "description": "Returns the character corresponding to a Unicode code point",
                    "parameters": [{"name": "n", "type": "number"}],
                    "return_type": "string",
                    "example": "chr(65) // returns \"A\""
                }"#
            ),
            (
                "ord",
                r#"{
                    "name": "ord",
                    "description": "Returns the Unicode code point of the first character in a string",
                    "parameters": [{"name": "str", "type": "string"}],
                    "return_type": "number",
                    "example": "ord(\"A\") // returns 65"
                }"#
            ),
            (
                "base",
                r#"{
                    "name": "base",
                    "description": "Converts an integer to a string in the given base (2-36)",
                    "parameters": [
                        {"name": "number", "type": "int"},
                        {"name": "target_base", "type": "int (2-36)"}
                    ],
                    "return_type": "string",
                    "example": "base(10, 2) // returns \"1010\""
                }"#
            ),
            (
                "available_functions",
                r#"{
                    "name": "available_functions",
                    "description": "Returns detailed information about any built-in function",
                    "parameters": [{"name": "function_name", "type": "string"}],
                    "return_type": "string (JSON with function info)",
                    "example": "available_functions(\"strlen\") // returns JSON info about strlen"
                }"#
            ),
        ];

        #[cfg(gui)]
        {
            functions.push((
                "Frame",
                r#"{
                    "name": "Frame",
                    "description": "Creates a GUI window frame",
                    "parameters": [
                        {"name": "title", "type": "string"},
                        {"name": "width", "type": "number"},
                        {"name": "height", "type": "number"},
                        {"name": "x", "type": "number"},
                        {"name": "y", "type": "number"}
                    ],
                    "return_type": "GUI Frame object",
                    "example": "$win = Frame(\"My App\", 800, 600, 100, 100)"
                }"#
            ));
        
            functions.push((
                "Label",
                r#"{
                    "name": "Label",
                    "description": "Creates a text label widget on a frame",
                    "parameters": [
                        {"name": "frame", "type": "GUI Frame"},
                        {"name": "text", "type": "string"},
                        {"name": "x", "type": "number"},
                        {"name": "y", "type": "number"},
                        {"name": "font_size", "type": "number"}
                    ],
                    "return_type": "GUI Widget",
                    "example": "$label = Label(win, \"Hello\", 10, 10, 14)"
                }"#
            ));
        
            functions.push((
                "Button",
                r#"{
                    "name": "Button",
                    "description": "Creates a button widget on a frame",
                    "parameters": [
                        {"name": "frame", "type": "GUI Frame"},
                        {"name": "text", "type": "string"},
                        {"name": "x", "type": "number"},
                        {"name": "y", "type": "number"},
                        {"name": "width", "type": "number"},
                        {"name": "height", "type": "number"}
                    ],
                    "return_type": "GUI Widget",
                    "example": "$btn = Button(win, \"Click Me\", 10, 50, 100, 30)"
                }"#
            ));
        
            functions.push((
                "button_on_click",
                r#"{
                    "name": "button_on_click",
                    "description": "Sets a callback function for a button click event",
                    "parameters": [
                        {"name": "button", "type": "GUI Widget"},
                        {"name": "callback", "type": "string (function call string)"}
                    ],
                    "return_type": "nil",
                    "example": "button_on_click(btn, \"my_function()\")"
                }"#
            ));
        
            functions.push((
                "auto_widget_scale",
                r#"{
                    "name": "auto_widget_scale",
                    "description": "Enables or disables automatic widget scaling on a frame",
                    "parameters": [
                        {"name": "frame", "type": "GUI Frame"},
                        {"name": "enabled", "type": "boolean"}
                    ],
                    "return_type": "nil",
                    "example": "auto_widget_scale(win, true)"
                }"#
            ));
        
            functions.push((
                "gui_start",
                r#"{
                    "name": "gui_start",
                    "description": "Starts the GUI main loop (blocks until window is closed)",
                    "parameters": [{"name": "frame", "type": "GUI Frame"}],
                    "return_type": "nil",
                    "example": "gui_start(win)"
                }"#
            ));
        
            functions.push((
                "gui_quit",
                r#"{
                    "name": "gui_quit",
                    "description": "Quits the GUI application",
                    "parameters": [],
                    "return_type": "nil",
                    "example": "gui_quit()"
                }"#
            ));
        }

        for (name, info) in &functions {
            if name == &func_name {
                let info_json = info.to_string();
                return Ok(Value::String(info_json));
            }
        }

        let mut available: Vec<String> = Vec::new();
        for (name, _) in &functions {
            available.push(name.to_string());
        }

        Err(format!(
            "Function '{}' not found. Available functions:\n{}",
            func_name,
            available.join(", ")
        ))
    }

    fn builtin_base(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("base expects exactly 2 arguments (number, target_base)".to_string());
        }
    
        let num = match interpreter.evaluate_expression(args[0].clone())? {
            Value::Number(n) => n,
            _ => return Err("base: first argument must be a number".to_string()),
        };
    
        let target_base = match interpreter.evaluate_expression(args[1].clone())? {
            Value::Number(n) => n as u32,
            _ => return Err("base: second argument must be a number".to_string()),
        };
    
        if target_base < 2 || target_base > 36 {
            return Err("base: target base must be between 2 and 36".to_string());
        }
    
        if num.fract() != 0.0 {
            return Err("base: only integers are supported".to_string());
        }
    
        let n = num as i64;
        let is_negative = n < 0;
        let mut abs_n = n.unsigned_abs();
    
        if abs_n == 0 {
            return Ok(Value::String("0".to_string()));
        }
    
        let digits = b"0123456789abcdefghijklmnopqrstuvwxyz";
        let mut result = Vec::new();
    
        while abs_n > 0 {
            let digit = (abs_n % target_base as u64) as usize;
            result.push(digits[digit] as char);
            abs_n /= target_base as u64;
        }
    
        if is_negative {
            result.push('-');
        }
    
        result.reverse();
        Ok(Value::String(result.into_iter().collect()))
    }

    fn builtin_range(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() < 2 || args.len() > 3 {
            return Err("range expects 2-3 arguments (start, end, [step])".to_string());
        }

        let start = match interpreter.evaluate_expression(args[0].clone())? {
            Value::Number(n) => n as i32,
            _ => return Err("range: start must be a number".to_string()),
        };

        let end = match interpreter.evaluate_expression(args[1].clone())? {
            Value::Number(n) => n as i32,
            _ => return Err("range: end must be a number".to_string()),
        };

        let step = if args.len() == 3 {
            match interpreter.evaluate_expression(args[2].clone())? {
                Value::Number(n) => n as i32,
                _ => return Err("range: step must be a number".to_string()),
            }
        } else {
            1
        };

        let result_ptr = unsafe {
            range(start, end, step)
        };

        if result_ptr.is_null() {
            return Err("range failed".to_string());
        }

        let result_str = unsafe {
            let c_str = std::ffi::CStr::from_ptr(result_ptr as *const c_char);
            c_str.to_string_lossy().into_owned()
        };

        unsafe { free_string(result_ptr) };

        let trimmed = result_str.trim();

        if trimmed == "[null]" || trimmed == "[]" {
            return Ok(Value::Array(Vec::new()));
        }

        if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
            return Ok(Value::String(trimmed.to_string()));
        }

        let inner = &trimmed[1..trimmed.len()-1].trim();
        if inner.is_empty() {
            return Ok(Value::Array(Vec::new()));
        }

        let mut arr = Vec::new();
        for part in inner.split(',').map(|s| s.trim()) {
            if !part.is_empty() {
                if let Ok(num) = part.parse::<f64>() {
                    arr.push(Value::Number(num));
                } else {
                    arr.push(Value::String(part.to_string()));
                }
            }
        }

        Ok(Value::Array(arr))
    }

    fn builtin_uuid_v4(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if !args.is_empty() {
            return Err("uuid_v4 expects no arguments".to_string());
        }

        let result_ptr = unsafe {
            uuid_v4()
        };

        if result_ptr.is_null() {
            return Err("uuid_v4 generation failed".to_string());
        }

        let result_str = unsafe {
            let c_str = std::ffi::CStr::from_ptr(result_ptr as *const c_char);
            c_str.to_string_lossy().into_owned()
        };

        unsafe { free_string(result_ptr) };

        Ok(Value::String(result_str))
    }
    fn builtin_base64_encode(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("base64_encode expects exactly 1 argument".to_string());
        }

        let input = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("base64_encode: argument must be a string".to_string()),
        };

        let chars = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let bytes = input.as_bytes();
        let mut result = String::with_capacity((bytes.len() + 2) / 3 * 4);
        let mut i = 0;

        while i + 3 <= bytes.len() {
            let (b1, b2, b3) = (bytes[i], bytes[i+1], bytes[i+2]);
            result.push(chars[(b1 >> 2) as usize] as char);
            result.push(chars[((b1 & 0x03) << 4 | (b2 >> 4)) as usize] as char);
            result.push(chars[((b2 & 0x0F) << 2 | (b3 >> 6)) as usize] as char);
            result.push(chars[(b3 & 0x3F) as usize] as char);
            i += 3;
        }

        if i < bytes.len() {
            let b1 = bytes[i];
            result.push(chars[(b1 >> 2) as usize] as char);
            if i + 1 < bytes.len() {
                let b2 = bytes[i + 1];
                result.push(chars[((b1 & 0x03) << 4 | (b2 >> 4)) as usize] as char);
                result.push(chars[((b2 & 0x0F) << 2) as usize] as char);
                result.push('=');
            } else {
                result.push(chars[((b1 & 0x03) << 4) as usize] as char);
                result.push('=');
                result.push('=');
            }
        }

        Ok(Value::String(result))
    }

    fn builtin_base64_decode(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("base64_decode expects exactly 1 argument".to_string());
        }

        let input = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("base64_decode: argument must be a string".to_string()),
        };

        let input: String = input.chars().filter(|c| !c.is_whitespace()).collect();
        if input.is_empty() {
            return Ok(Value::String(String::new()));
        }

        let chars = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut dec = [-1i8; 256];
        for (i, &c) in chars.iter().enumerate() {
            dec[c as usize] = i as i8;
        }
        dec[b'=' as usize] = -2;

        let mut bytes = Vec::with_capacity(input.len() / 4 * 3);
        let mut buf = 0u32;
        let mut bits = 0;

        for &c in input.as_bytes() {
            let v = dec[c as usize];
            if v == -2 { break; }
            if v < 0 { return Err(format!("Invalid Base64 char: '{}'", c as char)); }

            buf = (buf << 6) | (v as u32);
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                bytes.push(((buf >> bits) & 0xFF) as u8);
            }
        }

        match String::from_utf8(bytes) {
            Ok(s) => Ok(Value::String(s)),
            Err(e) => Err(format!("Invalid UTF-8: {}", e)),
        }
    }
    fn builtin_min(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.is_empty() {
            return Err("min expects at least 1 argument".to_string());
        }

        let mut min_val = interpreter.evaluate_expression(args[0].clone())?;
        let mut min_num = match &min_val {
            Value::Number(n) => *n,
            _ => return Err("min: all arguments must be numbers".to_string()),
        };

        for arg in args.iter().skip(1) {
            let val = interpreter.evaluate_expression(arg.clone())?;
            match val {
                Value::Number(n) => {
                    if n < min_num {
                        min_num = n;
                        min_val = val;
                    }
                }
                _ => return Err("min: all arguments must be numbers".to_string()),
            }
        }

        Ok(min_val)
    }

    fn builtin_max(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.is_empty() {
            return Err("max expects at least 1 argument".to_string());
        }

        let mut max_val = interpreter.evaluate_expression(args[0].clone())?;
        let mut max_num = match &max_val {
            Value::Number(n) => *n,
            _ => return Err("max: all arguments must be numbers".to_string()),
        };

        for arg in args.iter().skip(1) {
            let val = interpreter.evaluate_expression(arg.clone())?;
            match val {
                Value::Number(n) => {
                    if n > max_num {
                        max_num = n;
                        max_val = val;
                    }
                }
                _ => return Err("max: all arguments must be numbers".to_string()),
            }
        }

        Ok(max_val)
    }
    fn builtin_sin(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("sin expects exactly one argument".to_string());
        }
        let val = interpreter.evaluate_expression(args[0].clone())?;
        match val {
            Value::Number(x) => {
                let result = unsafe { sin_f(x) };
                Ok(Value::Number(result))
            }
            _ => Err("sin expects a numeric argument".to_string()),
        }
    }

    fn builtin_cos(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("cos expects exactly one argument".to_string());
        }
        let val = interpreter.evaluate_expression(args[0].clone())?;
        match val {
            Value::Number(x) => {
                let result = unsafe { cos_f(x) };
                Ok(Value::Number(result))
            }
            _ => Err("cos expects a numeric argument".to_string()),
        }
    }

    fn builtin_system(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("system expects exactly one argument (command string)".to_string());
        }

        let val = interpreter.evaluate_expression(args[0].clone())?;
        match val {
            Value::String(cmd) => {
                let result = unsafe { system_f(cmd.as_ptr() as *const i8) };
                Ok(Value::Number(result as f64))
            }
            _ => Err("system expects a string argument".to_string()),
        }
    }

    fn builtin_tan(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("tan expects exactly one argument".to_string());
        }
        let val = interpreter.evaluate_expression(args[0].clone())?;
        match val {
            Value::Number(x) => {
                let result = unsafe { tan_f(x) };
                Ok(Value::Number(result))
            }
            _ => Err("tan expects a numeric argument".to_string()),
        }
    }

    fn builtin_asin(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("asin expects exactly one argument".to_string());
        }
        let val = interpreter.evaluate_expression(args[0].clone())?;
        match val {
            Value::Number(x) => {
                if x < -1.0 || x > 1.0 {
                    return Err("asin argument must be between -1 and 1".to_string());
                }
                let result = unsafe { asin_f(x) };
                Ok(Value::Number(result))
            }
            _ => Err("asin expects a numeric argument".to_string()),
        }
    }

    fn builtin_acos(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("acos expects exactly one argument".to_string());
        }
        let val = interpreter.evaluate_expression(args[0].clone())?;
        match val {
            Value::Number(x) => {
                if x < -1.0 || x > 1.0 {
                    return Err("acos argument must be between -1 and 1".to_string());
                }
                let result = unsafe { acos_f(x) };
                Ok(Value::Number(result))
            }
            _ => Err("acos expects a numeric argument".to_string()),
        }
    }

    fn builtin_atan(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("atan expects exactly one argument".to_string());
        }
        let val = interpreter.evaluate_expression(args[0].clone())?;
        match val {
            Value::Number(x) => {
                let result = unsafe { atan_f(x) };
                Ok(Value::Number(result))
            }
            _ => Err("atan expects a numeric argument".to_string()),
        }
    }

    fn builtin_atan2(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("atan2 expects exactly two arguments (y, x)".to_string());
        }
        let y_val = interpreter.evaluate_expression(args[0].clone())?;
        let x_val = interpreter.evaluate_expression(args[1].clone())?;
        match (y_val, x_val) {
            (Value::Number(y), Value::Number(x)) => {
                let result = unsafe { atan2_f(y, x) };
                Ok(Value::Number(result))
            }
            _ => Err("atan2 expects numeric arguments".to_string()),
        }
    }

    fn builtin_csc(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("csc expects exactly one argument".to_string());
        }
        let val = interpreter.evaluate_expression(args[0].clone())?;
        match val {
            Value::Number(x) => {
                let sin_val = unsafe { sin_f(x) };
                if sin_val == 0.0 {
                    return Err("csc undefined for x where sin(x) = 0".to_string());
                }
                Ok(Value::Number(1.0 / sin_val))
            }
            _ => Err("csc expects a numeric argument".to_string()),
        }
    }

    fn builtin_sec(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("sec expects exactly one argument".to_string());
        }
        let val = interpreter.evaluate_expression(args[0].clone())?;
        match val {
            Value::Number(x) => {
                let cos_val = unsafe { cos_f(x) };
                if cos_val == 0.0 {
                    return Err("sec undefined for x where cos(x) = 0".to_string());
                }
                Ok(Value::Number(1.0 / cos_val))
            }
            _ => Err("sec expects a numeric argument".to_string()),
        }
    }

    fn builtin_cot(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("cot expects exactly one argument".to_string());
        }
        let val = interpreter.evaluate_expression(args[0].clone())?;
        match val {
            Value::Number(x) => {
                let tan_val = unsafe { tan_f(x) };
                if tan_val == 0.0 {
                    return Err("cot undefined for x where tan(x) = 0".to_string());
                }
                Ok(Value::Number(1.0 / tan_val))
            }
            _ => Err("cot expects a numeric argument".to_string()),
        }
    }

    fn builtin_sinh(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("sinh expects exactly one argument".to_string());
        }
        let val = interpreter.evaluate_expression(args[0].clone())?;
        match val {
            Value::Number(x) => {
                let result = unsafe { sinh_f(x) };
                Ok(Value::Number(result))
            }
            _ => Err("sinh expects a numeric argument".to_string()),
        }
    }

    fn builtin_cosh(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("cosh expects exactly one argument".to_string());
        }
        let val = interpreter.evaluate_expression(args[0].clone())?;
        match val {
            Value::Number(x) => {
                let result = unsafe { cosh_f(x) };
                Ok(Value::Number(result))
            }
            _ => Err("cosh expects a numeric argument".to_string()),
        }
    }

    fn builtin_tanh(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("tanh expects exactly one argument".to_string());
        }
        let val = interpreter.evaluate_expression(args[0].clone())?;
        match val {
            Value::Number(x) => {
                let result = unsafe { tanh_f(x) };
                Ok(Value::Number(result))
            }
            _ => Err("tanh expects a numeric argument".to_string()),
        }
    }

    fn builtin_asinh(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("asinh expects exactly one argument".to_string());
        }
        let val = interpreter.evaluate_expression(args[0].clone())?;
        match val {
            Value::Number(x) => {
                let result = unsafe { asinh_f(x) };
                Ok(Value::Number(result))
            }
            _ => Err("asinh expects a numeric argument".to_string()),
        }
    }

    fn builtin_acosh(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("acosh expects exactly one argument".to_string());
        }
        let val = interpreter.evaluate_expression(args[0].clone())?;
        match val {
            Value::Number(x) => {
                if x < 1.0 {
                    return Err("acosh argument must be >= 1".to_string());
                }
                let result = unsafe { acosh_f(x) };
                Ok(Value::Number(result))
            }
            _ => Err("acosh expects a numeric argument".to_string()),
        }
    }

    fn builtin_atanh(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("atanh expects exactly one argument".to_string());
        }
        let val = interpreter.evaluate_expression(args[0].clone())?;
        match val {
            Value::Number(x) => {
                if x <= -1.0 || x >= 1.0 {
                    return Err("atanh argument must be between -1 and 1 (exclusive)".to_string());
                }
                let result = unsafe { atanh_f(x) };
                Ok(Value::Number(result))
            }
            _ => Err("atanh expects a numeric argument".to_string()),
        }
    }

    fn builtin_exp(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("exp expects exactly one argument".to_string());
        }
        let val = interpreter.evaluate_expression(args[0].clone())?;
        match val {
            Value::Number(x) => {
                let result = unsafe { exp_f(x) };
                Ok(Value::Number(result))
            }
            _ => Err("exp expects a numeric argument".to_string()),
        }
    }

    fn builtin_log(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("log expects exactly one argument".to_string());
        }
        let val = interpreter.evaluate_expression(args[0].clone())?;
        match val {
            Value::Number(x) => {
                if x <= 0.0 {
                    return Err("log argument must be positive".to_string());
                }
                let result = unsafe { log_f(x) };
                Ok(Value::Number(result))
            }
            _ => Err("log expects a numeric argument".to_string()),
        }
    }

    fn builtin_log10(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("log10 expects exactly one argument".to_string());
        }
        let val = interpreter.evaluate_expression(args[0].clone())?;
        match val {
            Value::Number(x) => {
                if x <= 0.0 {
                    return Err("log10 argument must be positive".to_string());
                }
                let result = unsafe { log10_f(x) };
                Ok(Value::Number(result))
            }
            _ => Err("log10 expects a numeric argument".to_string()),
        }
    }

    fn builtin_log2(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("log2 expects exactly one argument".to_string());
        }
        let val = interpreter.evaluate_expression(args[0].clone())?;
        match val {
            Value::Number(x) => {
                if x <= 0.0 {
                    return Err("log2 argument must be positive".to_string());
                }
                let result = unsafe { log2_f(x) };
                Ok(Value::Number(result))
            }
            _ => Err("log2 expects a numeric argument".to_string()),
        }
    }

    fn builtin_pow(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("pow expects exactly two arguments (base, exponent)".to_string());
        }
        let base_val = interpreter.evaluate_expression(args[0].clone())?;
        let exp_val = interpreter.evaluate_expression(args[1].clone())?;
        match (base_val, exp_val) {
            (Value::Number(base), Value::Number(exp)) => {
                let result = unsafe { pow_f(base, exp) };
                Ok(Value::Number(result))
            }
            _ => Err("pow expects numeric arguments".to_string()),
        }
    }

    fn builtin_sqrt(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("sqrt expects exactly one argument".to_string());
        }
        let val = interpreter.evaluate_expression(args[0].clone())?;
        match val {
            Value::Number(x) => {
                if x < 0.0 {
                    return Err("sqrt argument must be non-negative".to_string());
                }
                let result = unsafe { sqrt_f(x) };
                Ok(Value::Number(result))
            }
            _ => Err("sqrt expects a numeric argument".to_string()),
        }
    }

    fn builtin_cbrt(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("cbrt expects exactly one argument".to_string());
        }
        let val = interpreter.evaluate_expression(args[0].clone())?;
        match val {
            Value::Number(x) => {
                let result = unsafe { cbrt_f(x) };
                Ok(Value::Number(result))
            }
            _ => Err("cbrt expects a numeric argument".to_string()),
        }
    }

    fn builtin_hypot(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("hypot expects exactly two arguments (x, y)".to_string());
        }
        let x_val = interpreter.evaluate_expression(args[0].clone())?;
        let y_val = interpreter.evaluate_expression(args[1].clone())?;
        match (x_val, y_val) {
            (Value::Number(x), Value::Number(y)) => {
                let result = unsafe { hypot_f(x, y) };
                Ok(Value::Number(result))
            }
            _ => Err("hypot expects numeric arguments".to_string()),
        }
    }

    fn builtin_factorial(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("factorial expects exactly 1 argument (n)".to_string());
        }
    
        let n = match interpreter.evaluate_expression(args[0].clone())? {
            Value::Number(num) => num,
            _ => return Err("factorial: argument must be a number".to_string()),
        };
    
        if n < 0.0 || n.fract() != 0.0 {
            return Err("factorial: argument must be a non-negative integer".to_string());
        }
    
        if n > 170.0 {
            return Err("factorial: argument too large (max 170)".to_string());
        }
    
        let result = unsafe { factorial_f(n as i32) };
    
        if result < 0.0 {
            return Err("factorial: calculation error".to_string());
        }
    
        Ok(Value::Number(result))
    }
    
    fn builtin_permutation(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("permutation expects exactly 2 arguments (n, r)".to_string());
        }
    
        let n = match interpreter.evaluate_expression(args[0].clone())? {
            Value::Number(num) => num,
            _ => return Err("permutation: n must be a number".to_string()),
        };
    
        let r = match interpreter.evaluate_expression(args[1].clone())? {
            Value::Number(num) => num,
            _ => return Err("permutation: r must be a number".to_string()),
        };
    
        if n < 0.0 || r < 0.0 || n.fract() != 0.0 || r.fract() != 0.0 {
            return Err("permutation: n and r must be non-negative integers".to_string());
        }
    
        if r > n {
            return Err("permutation: r must be less than or equal to n".to_string());
        }
    
        let result = unsafe { permutation_f(n as i32, r as i32) };
    
        if result < 0.0 {
            return Err("permutation: calculation error".to_string());
        }
    
        Ok(Value::Number(result))
    }
    
    fn builtin_combination(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("combination expects exactly 2 arguments (n, r)".to_string());
        }
    
        let n = match interpreter.evaluate_expression(args[0].clone())? {
            Value::Number(num) => num,
            _ => return Err("combination: n must be a number".to_string()),
        };
    
        let r = match interpreter.evaluate_expression(args[1].clone())? {
            Value::Number(num) => num,
            _ => return Err("combination: r must be a number".to_string()),
        };
    
        if n < 0.0 || r < 0.0 || n.fract() != 0.0 || r.fract() != 0.0 {
            return Err("combination: n and r must be non-negative integers".to_string());
        }
    
        if r > n {
            return Err("combination: r must be less than or equal to n".to_string());
        }
    
        let result = unsafe { combination_f(n as i32, r as i32) };
    
        if result < 0.0 {
            return Err("combination: calculation error".to_string());
        }
    
        Ok(Value::Number(result))
    }
    
    fn builtin_gcd(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("gcd expects exactly 2 arguments (a, b)".to_string());
        }
    
        let a = match interpreter.evaluate_expression(args[0].clone())? {
            Value::Number(num) => num,
            _ => return Err("gcd: a must be a number".to_string()),
        };
    
        let b = match interpreter.evaluate_expression(args[1].clone())? {
            Value::Number(num) => num,
            _ => return Err("gcd: b must be a number".to_string()),
        };
    
        if a.fract() != 0.0 || b.fract() != 0.0 {
            return Err("gcd: arguments must be integers".to_string());
        }
    
        let result = unsafe { gcd_ll(a as i64, b as i64) };
    
        Ok(Value::Number(result as f64))
    }
    
    fn builtin_lcm(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("lcm expects exactly 2 arguments (a, b)".to_string());
        }
    
        let a = match interpreter.evaluate_expression(args[0].clone())? {
            Value::Number(num) => num,
            _ => return Err("lcm: a must be a number".to_string()),
        };
    
        let b = match interpreter.evaluate_expression(args[1].clone())? {
            Value::Number(num) => num,
            _ => return Err("lcm: b must be a number".to_string()),
        };
    
        if a.fract() != 0.0 || b.fract() != 0.0 {
            return Err("lcm: arguments must be integers".to_string());
        }
    
        let result = unsafe { lcm_ll(a as i64, b as i64) };
    
        Ok(Value::Number(result as f64))
    }

    fn builtin_abs(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("abs expects exactly one argument".to_string());
        }
        let val = interpreter.evaluate_expression(args[0].clone())?;
        match val {
            Value::Number(x) => {
                let result = unsafe { abs_f(x) };
                Ok(Value::Number(result))
            }
            _ => Err("abs expects a numeric argument".to_string()),
        }
    }

    fn builtin_ceil(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("ceil expects exactly one argument".to_string());
        }
        let val = interpreter.evaluate_expression(args[0].clone())?;
        match val {
            Value::Number(x) => {
                let result = unsafe { ceil_f(x) };
                Ok(Value::Number(result))
            }
            _ => Err("ceil expects a numeric argument".to_string()),
        }
    }

    fn builtin_floor(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("floor expects exactly one argument".to_string());
        }
        let val = interpreter.evaluate_expression(args[0].clone())?;
        match val {
            Value::Number(x) => {
                let result = unsafe { floor_f(x) };
                Ok(Value::Number(result))
            }
            _ => Err("floor expects a numeric argument".to_string()),
        }
    }

    fn builtin_round(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("round expects exactly one argument".to_string());
        }
        let val = interpreter.evaluate_expression(args[0].clone())?;
        match val {
            Value::Number(x) => {
                let result = unsafe { round_f(x) };
                Ok(Value::Number(result))
            }
            _ => Err("round expects a numeric argument".to_string()),
        }
    }

    fn builtin_trunc(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("trunc expects exactly one argument".to_string());
        }
        let val = interpreter.evaluate_expression(args[0].clone())?;
        match val {
            Value::Number(x) => {
                let result = unsafe { trunc_f(x) };
                Ok(Value::Number(result))
            }
            _ => Err("trunc expects a numeric argument".to_string()),
        }
    }

    fn builtin_erf(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("erf expects exactly one argument".to_string());
        }
        let val = interpreter.evaluate_expression(args[0].clone())?;
        match val {
            Value::Number(x) => {
                let result = unsafe { erf_f(x) };
                Ok(Value::Number(result))
            }
            _ => Err("erf expects a numeric argument".to_string()),
        }
    }

    fn builtin_erfc(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("erfc expects exactly one argument".to_string());
        }
        let val = interpreter.evaluate_expression(args[0].clone())?;
        match val {
            Value::Number(x) => {
                let result = unsafe { erfc_f(x) };
                Ok(Value::Number(result))
            }
            _ => Err("erfc expects a numeric argument".to_string()),
        }
    }

    fn builtin_gamma(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("gamma expects exactly one argument".to_string());
        }
        let val = interpreter.evaluate_expression(args[0].clone())?;
        match val {
            Value::Number(x) => {
                if x <= 0.0 && x == x.floor() {
                    return Err("gamma undefined for negative integers".to_string());
                }
                let result = unsafe { gamma_f(x) };
                Ok(Value::Number(result))
            }
            _ => Err("gamma expects a numeric argument".to_string()),
        }
    }

    fn builtin_pi(_interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if !args.is_empty() {
            return Err("pi expects no arguments".to_string());
        }
        let result = unsafe { pi_f() };
        Ok(Value::Number(result))
    }

    fn builtin_e(_interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if !args.is_empty() {
            return Err("e expects no arguments".to_string());
        }
        let result = unsafe { e_f() };
        Ok(Value::Number(result))
    }

    fn get_array_from_variable(&self, name: &str) -> Result<Vec<Value>, String> {
        let value = self.environment.borrow().get(name)
            .ok_or_else(|| format!("Variable '{}' not found", name))?;

        match value {
            Value::Array(arr) => Ok(arr),
            _ => Err(format!("Variable '{}' is not an array", name)),
        }
    }

    fn set_array_to_variable(&mut self, name: &str, arr: Vec<Value>) -> Result<(), String> {
        self.environment.borrow_mut().set(name.to_string(), Value::Array(arr))
    }

    fn set_array_to_parent(&mut self, name: &str, arr: Vec<Value>) -> Result<(), String> {
        self.environment.borrow_mut().set_parent_scope(name.to_string(), Value::Array(arr))
    }

    fn get_array_from_parent(&self, name: &str) -> Result<Vec<Value>, String> {
        let value = self.environment.borrow().get_parent_scope(name)
            .ok_or_else(|| format!("Parent variable '{}' not found", name))?;

        match value {
            Value::Array(arr) => Ok(arr),
            _ => Err(format!("Parent variable '{}' is not an array", name)),
        }
    }

    fn builtin_array_push(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() < 2 {
            return Err("array_push expects at least two arguments (array, value1, value2, ...)".to_string());
        }

        match &args[0] {
            Expr::StructAccess(obj, field_name) => {
                let obj_value = interpreter.evaluate_expression(*obj.clone())?;
                match obj_value {
                    Value::Struct(instance_id, mut fields) => {
                        let mut arr = if let Some(Value::Array(arr)) = fields.get(field_name) {
                            arr.clone()
                        } else {
                            return Err(format!("Field '{}' is not an array", field_name));
                        };

                        for i in 1..args.len() {
                            let value = interpreter.evaluate_expression(args[i].clone())?;
                            arr.push(value);
                        }
                        let new_len = arr.len();

                        fields.insert(field_name.clone(), Value::Array(arr));
                        let updated_struct = Value::Struct(instance_id, fields);

                        if let Expr::Variable(var_name) = &**obj {
                            interpreter.environment.borrow_mut().set(var_name.clone(), updated_struct)?;
                        }

                        Ok(Value::Number(new_len as f64))
                    }
                    _ => Err("Cannot access array on non-struct value".to_string()),
                }
            }
            Expr::Variable(name) => {
                let mut arr = interpreter.get_array_from_variable(name)?;
                for i in 1..args.len() {
                    let value = interpreter.evaluate_expression(args[i].clone())?;
                    arr.push(value);
                }
                let new_len = arr.len();
                interpreter.set_array_to_variable(name, arr)?;
                Ok(Value::Number(new_len as f64))
            }
            Expr::ParentAccess(var_name) => {
                let mut arr = interpreter.get_array_from_parent(var_name)?;
                for i in 1..args.len() {
                    let value = interpreter.evaluate_expression(args[i].clone())?;
                    arr.push(value);
                }
                let new_len = arr.len();
                interpreter.set_array_to_parent(var_name, arr)?;
                Ok(Value::Number(new_len as f64))
            }
            _ => Err("array_push expects array variable as first argument".to_string()),
        }
    }

    fn builtin_array_remove(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("array_remove expects exactly two arguments (array, index)".to_string());
        }

        match &args[0] {
            Expr::StructAccess(obj, field_name) => {
                let obj_value = interpreter.evaluate_expression(*obj.clone())?;
                match obj_value {
                    Value::Struct(instance_id, mut fields) => {
                        let mut arr = if let Some(Value::Array(arr)) = fields.get(field_name) {
                            arr.clone()
                        } else {
                            return Err(format!("Field '{}' is not an array", field_name));
                        };

                        let index_val = interpreter.evaluate_expression(args[1].clone())?;
                        match index_val {
                            Value::Number(idx) => {
                                let idx_usize = idx as usize;
                                if idx_usize >= arr.len() {
                                    return Err(format!("Index {} out of bounds", idx_usize));
                                }
                                let removed = arr.remove(idx_usize);

                                fields.insert(field_name.clone(), Value::Array(arr));
                                let updated_struct = Value::Struct(instance_id, fields);

                                if let Expr::Variable(var_name) = &**obj {
                                    interpreter.environment.borrow_mut().set(var_name.clone(), updated_struct)?;
                                }

                                Ok(removed)
                            }
                            _ => Err("array_remove expects numeric index".to_string()),
                        }
                    }
                    _ => Err("Cannot access array on non-struct value".to_string()),
                }
            }
            Expr::Variable(name) => {
                let mut arr = interpreter.get_array_from_variable(name)?;
                let index_val = interpreter.evaluate_expression(args[1].clone())?;
                match index_val {
                    Value::Number(idx) => {
                        let idx_usize = idx as usize;
                        if idx_usize >= arr.len() {
                            return Err(format!("Index {} out of bounds", idx_usize));
                        }
                        let removed = arr.remove(idx_usize);
                        interpreter.set_array_to_variable(name, arr)?;
                        Ok(removed)
                    }
                    _ => Err("array_remove expects numeric index".to_string()),
                }
            }
            Expr::ParentAccess(var_name) => {
                let mut arr = interpreter.get_array_from_parent(var_name)?;
                let index_val = interpreter.evaluate_expression(args[1].clone())?;
                match index_val {
                    Value::Number(idx) => {
                        let idx_usize = idx as usize;
                        if idx_usize >= arr.len() {
                            return Err(format!("Index {} out of bounds", idx_usize));
                        }
                        let removed = arr.remove(idx_usize);
                        interpreter.set_array_to_parent(var_name, arr)?;
                        Ok(removed)
                    }
                    _ => Err("array_remove expects numeric index".to_string()),
                }
            }
            _ => Err("array_remove expects array variable as first argument".to_string()),
        }
    }

    fn builtin_array_pop(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("array_pop expects exactly one argument (array)".to_string());
        }

        match &args[0] {
            Expr::StructAccess(obj, field_name) => {
                let obj_value = interpreter.evaluate_expression(*obj.clone())?;
                match obj_value {
                    Value::Struct(instance_id, mut fields) => {
                        let mut arr = if let Some(Value::Array(arr)) = fields.get(field_name) {
                            arr.clone()
                        } else {
                            return Err(format!("Field '{}' is not an array", field_name));
                        };

                        if arr.is_empty() {
                            return Err("Cannot pop from empty array".to_string());
                        }
                        let value = arr.pop().unwrap();

                        fields.insert(field_name.clone(), Value::Array(arr));
                        let updated_struct = Value::Struct(instance_id, fields);

                        if let Expr::Variable(var_name) = &**obj {
                            interpreter.environment.borrow_mut().set(var_name.clone(), updated_struct)?;
                        }

                        Ok(value)
                    }
                    _ => Err("Cannot access array on non-struct value".to_string()),
                }
            }
            Expr::Variable(name) => {
                let mut arr = interpreter.get_array_from_variable(name)?;
                if arr.is_empty() {
                    return Err("Cannot pop from empty array".to_string());
                }
                let value = arr.pop().unwrap();
                interpreter.set_array_to_variable(name, arr)?;
                Ok(value)
            }
            Expr::ParentAccess(var_name) => {
                let mut arr = interpreter.get_array_from_parent(var_name)?;
                if arr.is_empty() {
                    return Err("Cannot pop from empty array".to_string());
                }
                let value = arr.pop().unwrap();
                interpreter.set_array_to_parent(var_name, arr)?;
                Ok(value)
            }
            _ => Err("array_pop expects array variable as argument".to_string()),
        }
    }

    fn builtin_array_shift(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("array_shift expects exactly one argument (array)".to_string());
        }

        match &args[0] {
            Expr::Variable(name) => {
                let mut arr = interpreter.get_array_from_variable(name)?;
                if arr.is_empty() {
                    return Err("Cannot shift from empty array".to_string());
                }
                let value = arr.remove(0);
                interpreter.set_array_to_variable(name, arr)?;
                Ok(value)
            }
            Expr::ParentAccess(var_name) => {
                let mut arr = interpreter.get_array_from_parent(var_name)?;
                if arr.is_empty() {
                    return Err("Cannot shift from empty array".to_string());
                }
                let value = arr.remove(0);
                interpreter.set_array_to_parent(var_name, arr)?;
                Ok(value)
            }
            _ => Err("array_shift expects array variable as argument".to_string()),
        }
    }

    fn builtin_array_unshift(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() < 2 {
            return Err("array_unshift expects at least two arguments (array, value1, value2, ...)".to_string());
        }

        match &args[0] {
            Expr::Variable(name) => {
                let mut arr = interpreter.get_array_from_variable(name)?;
                let mut values_to_prepend = Vec::new();

                for i in 1..args.len() {
                    let value = interpreter.evaluate_expression(args[i].clone())?;
                    values_to_prepend.push(value);
                }

                for value in values_to_prepend.into_iter().rev() {
                    arr.insert(0, value);
                }

                let new_len = arr.len();
                interpreter.set_array_to_variable(name, arr)?;
                Ok(Value::Number(new_len as f64))
            }
            Expr::ParentAccess(var_name) => {
                let mut arr = interpreter.get_array_from_parent(var_name)?;
                let mut values_to_prepend = Vec::new();

                for i in 1..args.len() {
                    let value = interpreter.evaluate_expression(args[i].clone())?;
                    values_to_prepend.push(value);
                }

                for value in values_to_prepend.into_iter().rev() {
                    arr.insert(0, value);
                }

                let new_len = arr.len();
                interpreter.set_array_to_parent(var_name, arr)?;
                Ok(Value::Number(new_len as f64))
            }
            _ => Err("array_unshift expects array variable as first argument".to_string()),
        }
    }

    fn builtin_array_get(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("array_get expects exactly two arguments (array, index)".to_string());
        }

        let arr = match &args[0] {
            Expr::StructAccess(obj, field_name) => {
                let obj_value = interpreter.evaluate_expression(*obj.clone())?;
                match obj_value {
                    Value::Struct(_, fields) => {
                        if let Some(Value::Array(arr)) = fields.get(field_name) {
                            arr.clone()
                        } else {
                            return Err(format!("Field '{}' is not an array", field_name));
                        }
                    }
                    _ => return Err("Cannot access array on non-struct value".to_string()),
                }
            }
            Expr::Variable(name) => {
                interpreter.get_array_from_variable(name)?
            }
            Expr::ParentAccess(var_name) => {
                interpreter.get_array_from_parent(var_name)?
            }
            _ => return Err("array_get expects array variable as first argument".to_string()),
        };

        let index_val = interpreter.evaluate_expression(args[1].clone())?;
        match index_val {
            Value::Number(idx) => {
                let idx_usize = idx as usize;
                if idx_usize >= arr.len() {
                    return Err(format!("Index {} out of bounds (array length {})", idx_usize, arr.len()));
                }
                Ok(arr[idx_usize].clone())
            }
            _ => Err("array_get expects numeric index".to_string()),
        }
    }

    fn builtin_array_set(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 3 {
            return Err("array_set expects exactly three arguments (array, index, value)".to_string());
        }

        match &args[0] {
            Expr::StructAccess(obj, field_name) => {
                let obj_value = interpreter.evaluate_expression(*obj.clone())?;
                match obj_value {
                    Value::Struct(instance_id, mut fields) => {
                        let mut arr = if let Some(Value::Array(arr)) = fields.get(field_name) {
                            arr.clone()
                        } else {
                            return Err(format!("Field '{}' is not an array", field_name));
                        };

                        let index_val = interpreter.evaluate_expression(args[1].clone())?;
                        let value = interpreter.evaluate_expression(args[2].clone())?;

                        match index_val {
                            Value::Number(idx) => {
                                let idx_usize = idx as usize;
                                if idx_usize >= arr.len() {
                                    arr.resize(idx_usize + 1, Value::Nil);
                                }
                                arr[idx_usize] = value;

                                fields.insert(field_name.clone(), Value::Array(arr));
                                let updated_struct = Value::Struct(instance_id, fields);

                                if let Expr::Variable(var_name) = &**obj {
                                    interpreter.environment.borrow_mut().set(var_name.clone(), updated_struct)?;
                                }

                                Ok(Value::Nil)
                            }
                            _ => Err("array_set expects numeric index".to_string()),
                        }
                    }
                    _ => Err("Cannot access array on non-struct value".to_string()),
                }
            }
            Expr::Variable(name) => {
                let mut arr = interpreter.get_array_from_variable(name)?;
                let index_val = interpreter.evaluate_expression(args[1].clone())?;
                let value = interpreter.evaluate_expression(args[2].clone())?;

                match index_val {
                    Value::Number(idx) => {
                        let idx_usize = idx as usize;
                        if idx_usize >= arr.len() {
                            arr.resize(idx_usize + 1, Value::Nil);
                        }
                        arr[idx_usize] = value;
                        interpreter.set_array_to_variable(name, arr)?;
                        Ok(Value::Nil)
                    }
                    _ => Err("array_set expects numeric index".to_string()),
                }
            }
            Expr::ParentAccess(var_name) => {
                let mut arr = interpreter.get_array_from_parent(var_name)?;
                let index_val = interpreter.evaluate_expression(args[1].clone())?;
                let value = interpreter.evaluate_expression(args[2].clone())?;

                match index_val {
                    Value::Number(idx) => {
                        let idx_usize = idx as usize;
                        if idx_usize >= arr.len() {
                            arr.resize(idx_usize + 1, Value::Nil);
                        }
                        arr[idx_usize] = value;
                        interpreter.set_array_to_parent(var_name, arr)?;
                        Ok(Value::Nil)
                    }
                    _ => Err("array_set expects numeric index".to_string()),
                }
            }
            _ => Err("array_set expects array variable as first argument".to_string()),
        }
    }

    fn builtin_array_len(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("array_len expects exactly one argument (array)".to_string());
        }

        let arr = match &args[0] {
            Expr::StructAccess(obj, field_name) => {
                let obj_value = interpreter.evaluate_expression(*obj.clone())?;
                match obj_value {
                    Value::Struct(_, fields) => {
                        if let Some(Value::Array(arr)) = fields.get(field_name) {
                            arr.clone()
                        } else {
                            return Err(format!("Field '{}' is not an array", field_name));
                        }
                    }
                    _ => return Err("Cannot access array on non-struct value".to_string()),
                }
            }
            Expr::Variable(name) => {
                interpreter.get_array_from_variable(name)?
            }
            Expr::ParentAccess(var_name) => {
                interpreter.get_array_from_parent(var_name)?
            }
            _ => return Err("array_len expects array variable as argument".to_string()),
        };

        Ok(Value::Number(arr.len() as f64))
    }

    fn builtin_array_insert(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 3 {
            return Err("array_insert expects exactly three arguments (array, index, value)".to_string());
        }

        match &args[0] {
            Expr::Variable(name) => {
                let mut arr = interpreter.get_array_from_variable(name)?;
                let index_val = interpreter.evaluate_expression(args[1].clone())?;
                let value = interpreter.evaluate_expression(args[2].clone())?;

                match index_val {
                    Value::Number(idx) => {
                        let idx_usize = idx as usize;
                        if idx_usize > arr.len() {
                            return Err(format!("Index {} out of bounds (max {})", idx_usize, arr.len()));
                        }
                        arr.insert(idx_usize, value);
                        let new_len = arr.len();
                        interpreter.set_array_to_variable(name, arr)?;
                        Ok(Value::Number(new_len as f64))
                    }
                    _ => Err("array_insert expects numeric index".to_string()),
                }
            }
            Expr::ParentAccess(var_name) => {
                let mut arr = interpreter.get_array_from_parent(var_name)?;
                let index_val = interpreter.evaluate_expression(args[1].clone())?;
                let value = interpreter.evaluate_expression(args[2].clone())?;

                match index_val {
                    Value::Number(idx) => {
                        let idx_usize = idx as usize;
                        if idx_usize > arr.len() {
                            return Err(format!("Index {} out of bounds (max {})", idx_usize, arr.len()));
                        }
                        arr.insert(idx_usize, value);
                        let new_len = arr.len();
                        interpreter.set_array_to_parent(var_name, arr)?;
                        Ok(Value::Number(new_len as f64))
                    }
                    _ => Err("array_insert expects numeric index".to_string()),
                }
            }
            _ => Err("array_insert expects array variable as first argument".to_string()),
        }
    }

    fn builtin_array_contains(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("array_contains expects exactly two arguments (array, value)".to_string());
        }

        let arr = match &args[0] {
            Expr::Variable(name) => interpreter.get_array_from_variable(name)?,
            Expr::ParentAccess(var_name) => interpreter.get_array_from_parent(var_name)?,
            _ => return Err("array_contains expects array variable as first argument".to_string()),
        };

        let value = interpreter.evaluate_expression(args[1].clone())?;
        let contains = arr.contains(&value);
        Ok(Value::Boolean(contains))
    }

    fn builtin_array_index_of(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("array_index_of expects exactly two arguments (array, value)".to_string());
        }

        let arr = match &args[0] {
            Expr::Variable(name) => interpreter.get_array_from_variable(name)?,
            Expr::ParentAccess(var_name) => interpreter.get_array_from_parent(var_name)?,
            _ => return Err("array_index_of expects array variable as first argument".to_string()),
        };

        let value = interpreter.evaluate_expression(args[1].clone())?;
        match arr.iter().position(|v| v == &value) {
            Some(idx) => Ok(Value::Number(idx as f64)),
            None => Ok(Value::Number(-1.0)),
        }
    }

    fn builtin_array_last_index_of(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("array_last_index_of expects exactly two arguments (array, value)".to_string());
        }

        let arr = match &args[0] {
            Expr::Variable(name) => interpreter.get_array_from_variable(name)?,
            Expr::ParentAccess(var_name) => interpreter.get_array_from_parent(var_name)?,
            _ => return Err("array_last_index_of expects array variable as first argument".to_string()),
        };

        let value = interpreter.evaluate_expression(args[1].clone())?;
        match arr.iter().rposition(|v| v == &value) {
            Some(idx) => Ok(Value::Number(idx as f64)),
            None => Ok(Value::Number(-1.0)),
        }
    }

    fn builtin_array_min(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("array_min expects exactly 1 argument (array)".to_string());
        }

        let arr = match &args[0] {
            Expr::Variable(name) => interpreter.get_array_from_variable(name)?,
            Expr::ParentAccess(var_name) => interpreter.get_array_from_parent(var_name)?,
            _ => return Err("array_min: argument must be an array variable".to_string()),
        };

        if arr.is_empty() {
            return Err("array_min: array is empty".to_string());
        }

        let mut min_val = &arr[0];
        let mut min_num = match min_val {
            Value::Number(n) => *n,
            _ => return Err("array_min: array must contain numbers".to_string()),
        };

        for val in arr.iter().skip(1) {
            match val {
                Value::Number(n) => {
                    if *n < min_num {
                        min_num = *n;
                        min_val = val;
                    }
                }
                _ => return Err("array_min: array must contain numbers".to_string()),
            }
        }

        Ok(min_val.clone())
    }

    fn builtin_array_max(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("array_max expects exactly 1 argument (array)".to_string());
        }

        let arr = match &args[0] {
            Expr::Variable(name) => interpreter.get_array_from_variable(name)?,
            Expr::ParentAccess(var_name) => interpreter.get_array_from_parent(var_name)?,
            _ => return Err("array_max: argument must be an array variable".to_string()),
        };

        if arr.is_empty() {
            return Err("array_max: array is empty".to_string());
        }

        let mut max_val = &arr[0];
        let mut max_num = match max_val {
            Value::Number(n) => *n,
            _ => return Err("array_max: array must contain numbers".to_string()),
        };

        for val in arr.iter().skip(1) {
            match val {
                Value::Number(n) => {
                    if *n > max_num {
                        max_num = *n;
                        max_val = val;
                    }
                }
                _ => return Err("array_max: array must contain numbers".to_string()),
            }
        }

        Ok(max_val.clone())
    }

    fn builtin_array_unique(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("array_unique expects exactly 1 argument (array)".to_string());
        }

        let arr = match &args[0] {
            Expr::Variable(name) => interpreter.get_array_from_variable(name)?,
            Expr::ParentAccess(var_name) => interpreter.get_array_from_parent(var_name)?,
            _ => return Err("array_unique: argument must be an array variable".to_string()),
        };

        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();

        for value in arr {
            let key = value.to_string();
            if !seen.contains(&key) {
                seen.insert(key);
                result.push(value);
            }
        }

        Ok(Value::Array(result))
    }

    fn builtin_array_rand(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("array_rand expects exactly 1 argument (array)".to_string());
        }

        let arr = match &args[0] {
            Expr::Variable(name) => interpreter.get_array_from_variable(name)?,
            Expr::ParentAccess(var_name) => interpreter.get_array_from_parent(var_name)?,
            _ => return Err("array_rand: argument must be an array variable".to_string()),
        };

        if arr.is_empty() {
            return Err("array_rand: array is empty".to_string());
        }

        let random = Self::builtin_rand_float(interpreter, Vec::new())?;
        let index = match random {
            Value::Number(n) => (n * (arr.len() as f64)) as usize,
            _ => 0,
        };

        let idx = if index >= arr.len() { arr.len() - 1 } else { index };
        Ok(arr[idx].clone())
    }

    fn builtin_array_join(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("array_join expects exactly two arguments (array, separator)".to_string());
        }

        let arr = match &args[0] {
            Expr::Variable(name) => interpreter.get_array_from_variable(name)?,
            Expr::ParentAccess(var_name) => interpreter.get_array_from_parent(var_name)?,
            _ => return Err("array_join expects array variable as first argument".to_string()),
        };

        let sep_val = interpreter.evaluate_expression(args[1].clone())?;
        let separator = match sep_val {
            Value::String(s) => s,
            _ => sep_val.to_string(),
        };

        let strings: Vec<String> = arr.iter().map(|v| v.to_string()).collect();
        Ok(Value::String(strings.join(&separator)))
    }

    fn builtin_array_slice(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 3 {
            return Err("array_slice expects exactly three arguments (array, start, end)".to_string());
        }

        let arr = match &args[0] {
            Expr::Variable(name) => interpreter.get_array_from_variable(name)?,
            Expr::ParentAccess(var_name) => interpreter.get_array_from_parent(var_name)?,
            _ => return Err("array_slice expects array variable as first argument".to_string()),
        };

        let start_val = interpreter.evaluate_expression(args[1].clone())?;
        let end_val = interpreter.evaluate_expression(args[2].clone())?;

        match (start_val, end_val) {
            (Value::Number(start), Value::Number(end)) => {
                let start_idx = start as usize;
                let end_idx = end as usize;

                if start_idx > arr.len() || end_idx > arr.len() || start_idx > end_idx {
                    return Err("Invalid slice bounds".to_string());
                }

                let slice = arr[start_idx..end_idx].to_vec();
                Ok(Value::Array(slice))
            }
            _ => Err("array_slice expects numeric start and end".to_string()),
        }
    }

    fn builtin_array_splice(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 3 {
            return Err("array_splice expects exactly three arguments (array, start, length)".to_string());
        }

        match &args[0] {
            Expr::Variable(name) => {
                let mut arr = interpreter.get_array_from_variable(name)?;
                let start_val = interpreter.evaluate_expression(args[1].clone())?;
                let length_val = interpreter.evaluate_expression(args[2].clone())?;

                match (start_val, length_val) {
                    (Value::Number(start), Value::Number(length)) => {
                        let start_idx = start as usize;
                        let length_idx = length as usize;

                        if start_idx > arr.len() {
                            return Err("Start index out of bounds".to_string());
                        }

                        let end_idx = std::cmp::min(start_idx + length_idx, arr.len());
                        let removed: Vec<Value> = arr.drain(start_idx..end_idx).collect();

                        interpreter.set_array_to_variable(name, arr)?;
                        Ok(Value::Array(removed))
                    }
                    _ => Err("array_splice expects numeric start and length".to_string()),
                }
            }
            Expr::ParentAccess(var_name) => {
                let mut arr = interpreter.get_array_from_parent(var_name)?;
                let start_val = interpreter.evaluate_expression(args[1].clone())?;
                let length_val = interpreter.evaluate_expression(args[2].clone())?;

                match (start_val, length_val) {
                    (Value::Number(start), Value::Number(length)) => {
                        let start_idx = start as usize;
                        let length_idx = length as usize;

                        if start_idx > arr.len() {
                            return Err("Start index out of bounds".to_string());
                        }

                        let end_idx = std::cmp::min(start_idx + length_idx, arr.len());
                        let removed: Vec<Value> = arr.drain(start_idx..end_idx).collect();

                        interpreter.set_array_to_parent(var_name, arr)?;
                        Ok(Value::Array(removed))
                    }
                    _ => Err("array_splice expects numeric start and length".to_string()),
                }
            }
            _ => Err("array_splice expects array variable as first argument".to_string()),
        }
    }

    fn builtin_array_sum(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("array_sum expects exactly 1 argument (array)".to_string());
        }

        let arr = match &args[0] {
            Expr::Variable(name) => interpreter.get_array_from_variable(name)?,
            Expr::ParentAccess(var_name) => interpreter.get_array_from_parent(var_name)?,
            Expr::StructAccess(obj, field_name) => {
                let obj_value = interpreter.evaluate_expression(*obj.clone())?;
                match obj_value {
                    Value::Struct(_, fields) => {
                        if let Some(Value::Array(arr)) = fields.get(field_name) {
                            arr.clone()
                        } else {
                            return Err(format!("Field '{}' is not an array", field_name));
                        }
                    }
                    _ => return Err("Cannot access array on non-struct value".to_string()),
                }
            }
            _ => return Err("array_sum expects array variable as first argument".to_string()),
        };

        if arr.is_empty() {
            return Ok(Value::Number(0.0));
        }

        let mut sum = 0.0;
        let mut has_string = false;
        let mut string_parts = Vec::new();

        for val in arr {
            match val {
                Value::Number(n) => {
                    sum += n;
                }
                Value::String(s) => {
                    has_string = true;
                    string_parts.push(s);
                }
                Value::Char(c) => {
                    has_string = true;
                    string_parts.push(c.to_string());
                }
                _ => {
                    has_string = true;
                    string_parts.push(val.to_string());
                }
            }
        }

        if has_string {
            return Ok(Value::String(string_parts.join("")));
        }

        Ok(Value::Number(sum))
    }
    fn builtin_array_reverse(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("array_reverse expects exactly one argument (array)".to_string());
        }

        match &args[0] {
            Expr::Variable(name) => {
                let mut arr = interpreter.get_array_from_variable(name)?;
                arr.reverse();
                interpreter.set_array_to_variable(name, arr)?;
                Ok(Value::Nil)
            }
            Expr::ParentAccess(var_name) => {
                let mut arr = interpreter.get_array_from_parent(var_name)?;
                arr.reverse();
                interpreter.set_array_to_parent(var_name, arr)?;
                Ok(Value::Nil)
            }
            _ => Err("array_reverse expects array variable as argument".to_string()),
        }
    }

    fn builtin_array_sort(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() < 1 || args.len() > 2 {
            return Err("array_sort expects 1-2 arguments (array, [comparator])".to_string());
        }

        let arr = match &args[0] {
            Expr::Variable(name) => interpreter.get_array_from_variable(name)?,
            Expr::ParentAccess(var_name) => interpreter.get_array_from_parent(var_name)?,
            _ => return Err("array_sort expects array variable as first argument".to_string()),
        };

        let var_name = match &args[0] {
            Expr::Variable(name) => name.clone(),
            Expr::ParentAccess(var_name) => var_name.clone(),
            _ => return Err("array_sort: invalid array reference".to_string()),
        };

        if args.len() == 2 {
            let callback = args[1].clone();
            let callback_value = interpreter.evaluate_expression(callback)?;

            let func_rc = match callback_value {
                Value::Function(f) => f,
                _ => return Err("array_sort: second argument must be a lambda function".to_string()),
            };

            let mut sorted_arr = arr;
            let func_name = "__temp_comparator".to_string();
            let len = sorted_arr.len();

            for i in 0..len {
                for j in 0..(len - i - 1) {
                    let left_val = sorted_arr[j].clone();
                    let right_val = sorted_arr[j + 1].clone();

                    let left_literal = interpreter.value_to_literal(left_val)?;
                    let right_literal = interpreter.value_to_literal(right_val)?;

                    let call_args = vec![
                        Expr::Literal(left_literal),
                        Expr::Literal(right_literal)
                    ];

                    interpreter.environment.borrow_mut().define(func_name.clone(), Value::Function(func_rc.clone()));
                    let temp_call = Expr::Call(func_name.clone(), call_args);
                    let result = interpreter.evaluate_expression(temp_call)?;

                    let should_swap = match result {
                        Value::Number(n) => n > 0.0,
                        _ => !interpreter.is_truthy(&result),
                    };

                    if should_swap {
                        sorted_arr.swap(j, j + 1);
                    }
                }
            }

            match &args[0] {
                Expr::Variable(name) => interpreter.set_array_to_variable(name, sorted_arr)?,
                Expr::ParentAccess(var_name) => interpreter.set_array_to_parent(var_name, sorted_arr)?,
                _ => return Err("array_sort: invalid array reference".to_string()),
            }

            Ok(Value::Nil)
        } else {
            let mut sorted_arr = arr;
            sorted_arr.sort_by(|a, b| a.to_string().cmp(&b.to_string()));

            match &args[0] {
                Expr::Variable(name) => interpreter.set_array_to_variable(name, sorted_arr)?,
                Expr::ParentAccess(var_name) => interpreter.set_array_to_parent(var_name, sorted_arr)?,
                _ => return Err("array_sort: invalid array reference".to_string()),
            }

            Ok(Value::Nil)
        }
    }

    fn builtin_array_rsort(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() < 1 || args.len() > 2 {
            return Err("array_rsort expects 1-2 arguments (array, [comparator])".to_string());
        }

        let arr = match &args[0] {
            Expr::Variable(name) => interpreter.get_array_from_variable(name)?,
            Expr::ParentAccess(var_name) => interpreter.get_array_from_parent(var_name)?,
            _ => return Err("array_rsort expects array variable as first argument".to_string()),
        };

        let var_name = match &args[0] {
            Expr::Variable(name) => name.clone(),
            Expr::ParentAccess(var_name) => var_name.clone(),
            _ => return Err("array_rsort: invalid array reference".to_string()),
        };

        if args.len() == 2 {
            let callback = args[1].clone();
            let callback_value = interpreter.evaluate_expression(callback)?;

            let func_rc = match callback_value {
                Value::Function(f) => f,
                _ => return Err("array_rsort: second argument must be a lambda function".to_string()),
            };

            let mut sorted_arr = arr;
            let func_name = "__temp_comparator".to_string();

            let len = sorted_arr.len();
            for i in 0..len {
                for j in 0..(len - i - 1) {
                    let left_literal = interpreter.value_to_literal(sorted_arr[j + 1].clone())?;
                    let right_literal = interpreter.value_to_literal(sorted_arr[j].clone())?;

                    let call_args = vec![
                        Expr::Literal(left_literal),
                        Expr::Literal(right_literal)
                    ];

                    interpreter.environment.borrow_mut().define(func_name.clone(), Value::Function(func_rc.clone()));
                    let temp_call = Expr::Call(func_name.clone(), call_args);
                    let result = interpreter.evaluate_expression(temp_call)?;

                    if interpreter.is_truthy(&result) {
                        sorted_arr.swap(j, j + 1);
                    }
                }
            }

            match &args[0] {
                Expr::Variable(name) => interpreter.set_array_to_variable(name, sorted_arr)?,
                Expr::ParentAccess(var_name) => interpreter.set_array_to_parent(var_name, sorted_arr)?,
                _ => return Err("array_rsort: invalid array reference".to_string()),
            }

            Ok(Value::Nil)
        } else {
            let mut sorted_arr = arr;
            sorted_arr.sort_by(|a, b| b.to_string().cmp(&a.to_string()));

            match &args[0] {
                Expr::Variable(name) => interpreter.set_array_to_variable(name, sorted_arr)?,
                Expr::ParentAccess(var_name) => interpreter.set_array_to_parent(var_name, sorted_arr)?,
                _ => return Err("array_rsort: invalid array reference".to_string()),
            }

            Ok(Value::Nil)
        }
    }

   fn builtin_array_clear(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("array_clear expects exactly one argument (array)".to_string());
        }

        match &args[0] {
            Expr::Variable(name) => {
                interpreter.set_array_to_variable(name, Vec::new())?;
                Ok(Value::Nil)
            }
            Expr::ParentAccess(var_name) => {
                interpreter.set_array_to_parent(var_name, Vec::new())?;
                Ok(Value::Nil)
            }
            _ => Err("array_clear expects array variable as argument".to_string()),
        }
    }

    fn builtin_array_copy(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("array_copy expects exactly one argument (array)".to_string());
        }

        let arr = match &args[0] {
            Expr::Variable(name) => interpreter.get_array_from_variable(name)?,
            Expr::ParentAccess(var_name) => interpreter.get_array_from_parent(var_name)?,
            _ => return Err("array_copy expects array variable as argument".to_string()),
        };

        Ok(Value::Array(arr.clone()))
    }

    fn builtin_array_merge(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("array_merge expects exactly two arguments (array1, array2)".to_string());
        }

        let arr1 = match &args[0] {
            Expr::Variable(name) => interpreter.get_array_from_variable(name)?,
            Expr::ParentAccess(var_name) => interpreter.get_array_from_parent(var_name)?,
            _ => return Err("array_merge expects array variable as first argument".to_string()),
        };

        let arr2 = match &args[1] {
            Expr::Variable(name) => interpreter.get_array_from_variable(name)?,
            Expr::ParentAccess(var_name) => interpreter.get_array_from_parent(var_name)?,
            _ => return Err("array_merge expects array variable as second argument".to_string()),
        };

        let mut merged = arr1;
        merged.extend(arr2);
        Ok(Value::Array(merged))
    }

    fn builtin_array_map(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("array_map expects exactly 2 arguments (array, callback)".to_string());
        }

        let array_value = interpreter.evaluate_expression(args[0].clone())?;
        let arr = match array_value {
            Value::Array(a) => a,
            _ => return Err("array_map: first argument must be an array".to_string()),
        };

        let callback = args[1].clone();
        let callback_value = interpreter.evaluate_expression(callback)?;

        let func_rc = match callback_value {
            Value::Function(f) => f,
            _ => return Err("array_map: second argument must be a lambda function".to_string()),
        };

        let mut result = Vec::new();
        let func_name = "__temp_lambda".to_string();

        for item in arr {
            let item_literal = interpreter.value_to_literal(item.clone())?;
            let call_args = vec![Expr::Literal(item_literal)];

            interpreter.environment.borrow_mut().define(func_name.clone(), Value::Function(func_rc.clone()));
            let temp_call = Expr::Call(func_name.clone(), call_args);
            let value = interpreter.evaluate_expression(temp_call)?;
            result.push(value);
        }

        Ok(Value::Array(result))
    }

    fn builtin_array_filter(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("array_filter expects exactly 2 arguments (array, callback)".to_string());
        }

        let array_value = interpreter.evaluate_expression(args[0].clone())?;
        let arr = match array_value {
            Value::Array(a) => a,
            _ => return Err("array_filter: first argument must be an array".to_string()),
        };

        let callback = args[1].clone();
        let callback_value = interpreter.evaluate_expression(callback)?;

        let func_rc = match callback_value {
            Value::Function(f) => f,
            _ => return Err("array_filter: second argument must be a lambda function".to_string()),
        };

        let mut result = Vec::new();
        let func_name = "__temp_lambda".to_string();

        for item in arr {
            let item_literal = interpreter.value_to_literal(item.clone())?;
            let call_args = vec![Expr::Literal(item_literal)];

            interpreter.environment.borrow_mut().define(func_name.clone(), Value::Function(func_rc.clone()));
            let temp_call = Expr::Call(func_name.clone(), call_args);
            let value = interpreter.evaluate_expression(temp_call)?;

            if interpreter.is_truthy(&value) {
                result.push(item);
            }
        }

        Ok(Value::Array(result))
    }

    fn builtin_array_reduce(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 3 {
            return Err("array_reduce expects exactly 3 arguments (array, callback, initial)".to_string());
        }

        let array_value = interpreter.evaluate_expression(args[0].clone())?;
        let arr = match array_value {
            Value::Array(a) => a,
            _ => return Err("array_reduce: first argument must be an array".to_string()),
        };

        let initial = interpreter.evaluate_expression(args[2].clone())?;

        let callback = args[1].clone();
        let callback_value = interpreter.evaluate_expression(callback)?;

        let func_rc = match callback_value {
            Value::Function(f) => f,
            _ => return Err("array_reduce: second argument must be a lambda function".to_string()),
        };

        let mut acc = initial;
        let func_name = "__temp_lambda".to_string();

        for item in arr {
            let acc_literal = interpreter.value_to_literal(acc.clone())?;
            let item_literal = interpreter.value_to_literal(item.clone())?;

            let call_args = vec![
                Expr::Literal(acc_literal),
                Expr::Literal(item_literal)
            ];

            interpreter.environment.borrow_mut().define(func_name.clone(), Value::Function(func_rc.clone()));
            let temp_call = Expr::Call(func_name.clone(), call_args);
            acc = interpreter.evaluate_expression(temp_call)?;
        }

        Ok(acc)
    }
    fn builtin_terminal(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if !args.is_empty() {
            return Err("terminal expects no arguments".to_string());
        }

        let args_array: Vec<Value> = interpreter.terminal_args
            .iter()
            .map(|s| Value::String(s.clone()))
            .collect();

        Ok(Value::Array(args_array))
    }

    fn builtin_type_of_variable(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("type_of_variable expects exactly one argument".to_string());
        }

        let value = interpreter.evaluate_expression(args[0].clone())?;

        let type_name = match value {
            Value::Number(n) => {
                if n == n.floor() {
                    "int".to_string()
                } else {
                    "float".to_string()
                }
            }
            Value::String(_) => "string".to_string(),
            Value::Char(_) => "char".to_string(),
            Value::Boolean(_) => "boolean".to_string(),
            Value::Array(_) => "array".to_string(),
            Value::HashMap(_) => "map".to_string(),
            Value::Struct(name, _) => format!("struct({})", name),
            Value::Function(_) => "function".to_string(),
            Value::DatabaseHandle(_) => "database".to_string(), 
            Value::FileHandle(_) => "file".to_string(),
            Value::GUIFrame(_) => "gui_frame".to_string(),
            Value::GUIWidget(_) => "gui_widget".to_string(),
            Value::Enum(enum_name, variant) => format!("enum({}::{})", enum_name, variant),
            Value::Nil => "nil".to_string(),
        };

        Ok(Value::String(type_name))
    }

    fn builtin_int(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("int expects exactly one argument".to_string());
        }

        let value = interpreter.evaluate_expression(args[0].clone())?;

        match value {
            Value::Number(n) => {
                let int_val = n.floor();
                Ok(Value::Number(int_val))
            }
            Value::GUIFrame(_) | Value::GUIWidget(_) => {
                Err("Cannot convert GUI object to number/string".to_string())
            }
            Value::String(s) => {
                let trimmed = s.trim();
                let is_valid = trimmed.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '-');

                if !is_valid {
                    return Err(format!("String '{}' can't be converted to int", s));
                }

                match trimmed.parse::<f64>() {
                    Ok(num) => {
                        let int_val = num.floor();
                        Ok(Value::Number(int_val))
                    }
                    Err(_) => Err(format!("String '{}' can't be converted to int", s)),
                }
            }
            Value::Boolean(b) => {
                if b {
                    Ok(Value::Number(1.0))
                } else {
                    Ok(Value::Number(0.0))
                }
            }
            Value::Array(_) => Err("Cannot convert array to int".to_string()),
            Value::Function(_) => Err("Cannot convert function to int".to_string()),
            Value::FileHandle(_) => Err("Cannot convert file handle to int".to_string()),
            Value::Struct(_, _) => Err("Cannot convert struct to int".to_string()),
            Value::Enum(_, _) => Err("Cannot convert enum to int".to_string()),
            Value::HashMap(_) => Err("Cannot convert map to int".to_string()),
            Value::DatabaseHandle(_) => Err("Cannot convert database to int".to_string()),
            Value::Char(c) => {
                Ok(Value::Number(c as u32 as f64))
            }
            Value::Nil => Err("Cannot convert nil to int".to_string()),
        }
    }

    fn builtin_string(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("string expects exactly one argument".to_string());
        }

        let value = interpreter.evaluate_expression(args[0].clone())?;

        let str_val = match value {
            Value::Number(n) => {
                if n == n.floor() {
                    format!("{:.0}", n)
                } else {
                    n.to_string()
                }
            }
            Value::String(s) => s,
            Value::GUIFrame(_) => "<GUI Frame>".to_string(),
            Value::GUIWidget(_) => "<GUI Widget>".to_string(),
            Value::Char(c) => c.to_string(),
            Value::Boolean(b) => b.to_string(),
            Value::Array(arr) => {
                let mut result = String::from("[");
                for (i, v) in arr.iter().enumerate() {
                    if i > 0 {
                        result.push_str(", ");
                    }
                    result.push_str(&v.to_string());
                }
                result.push_str("]");
                result
            }
            Value::Struct(name, fields) => {
                let mut result = format!("{} {{ ", name);
                let mut first = true;
                for (k, v) in fields {
                    if !first {
                        result.push_str(", ");
                    }
                    result.push_str(&format!("${}: {}", k, v));
                    first = false;
                }
                result.push_str(" }");
                result
            }
            Value::Function(_) => "<function>".to_string(),
            Value::FileHandle(_) => "<file>".to_string(),
            Value::Enum(_, _) => "<enum>".to_string(),
            Value::DatabaseHandle(h) => format!("<database {}>", h),
            Value::HashMap(map) => {
                let mut result = String::from("{");
                let mut first = true;
                for (k, v) in map {
                    if !first {
                        result.push_str(", ");
                    }
                    result.push_str(&format!("\"{}\": {}", k, v));
                    first = false;
                }
                result.push_str("}");
                result
            }
            Value::Nil => "nil".to_string(),
        };

        Ok(Value::String(str_val))
    }

    fn builtin_float(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("float expects exactly one argument".to_string());
        }

        let value = interpreter.evaluate_expression(args[0].clone())?;

        match value {
            Value::Number(n) => {
                Ok(Value::Number(n))
            }
            Value::GUIFrame(_) | Value::GUIWidget(_) => {
                Err("Cannot convert GUI object to number/string".to_string())
            }
            Value::String(s) => {
                let trimmed = s.trim();
                let is_valid = trimmed.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '-');

                if !is_valid {
                    return Err(format!("String '{}' can't be converted to float", s));
                }

                match trimmed.parse::<f64>() {
                    Ok(num) => Ok(Value::Number(num)),
                    Err(_) => Err(format!("String '{}' can't be converted to float", s)),
                }
            }
            Value::Boolean(b) => {
                if b {
                    Ok(Value::Number(1.0))
                } else {
                    Ok(Value::Number(0.0))
                }
            }
            Value::Array(_) => Err("Cannot convert array to float".to_string()),
            Value::Function(_) => Err("Cannot convert function to float".to_string()),
            Value::FileHandle(_) => Err("Cannot convert file handle to float".to_string()),
            Value::Struct(_, _) => Err("Cannot convert struct to float".to_string()),
            Value::Enum(_, _) => Err("Cannot convert enum to float".to_string()),
            Value::HashMap(_) => Err("Cannot convert map to float".to_string()),
            Value::DatabaseHandle(_) => Err("Cannot convert database to float".to_string()),
            Value::Char(c) => {
                Ok(Value::Number(c as u32 as f64))
            }
            Value::Nil => Err("Cannot convert nil to float".to_string()),
        }
    }

    fn builtin_regex_test(&mut self, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 && args.len() != 3 {
            return Err("regex_test expects 2-3 arguments (pattern, text, [flags])".to_string());
        }

        let pattern = match self.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("regex_test: pattern must be a string".to_string()),
        };

        let text = match self.evaluate_expression(args[1].clone())? {
            Value::String(s) => s,
            _ => return Err("regex_test: text must be a string".to_string()),
        };

        let flags = if args.len() == 3 {
            match self.evaluate_expression(args[2].clone())? {
                Value::String(s) => Self::parse_regex_flags(&s),
                _ => 0,
            }
        } else {
            0
        };

        let result = unsafe {
            regex_test(pattern.as_ptr() as *const i8, text.as_ptr() as *const i8, flags)
        };

        if result < 0 {
            return Err("Regex error".to_string());
        }

        Ok(Value::Boolean(result == 1))
    }

    fn builtin_regex_find(&mut self, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 && args.len() != 3 && args.len() != 4 {
            return Err("regex_find expects 2-4 arguments (pattern, text, [group], [flags])".to_string());
        }

        let pattern = match self.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("regex_find: pattern must be a string".to_string()),
        };

        let text = match self.evaluate_expression(args[1].clone())? {
            Value::String(s) => s,
            _ => return Err("regex_find: text must be a string".to_string()),
        };

        let group = if args.len() >= 3 {
            match self.evaluate_expression(args[2].clone())? {
                Value::Number(n) => n as i32,
                _ => 0,
            }
        } else {
            0
        };

        let flags = if args.len() == 4 {
            match self.evaluate_expression(args[3].clone())? {
                Value::String(s) => Self::parse_regex_flags(&s),
                _ => 0,
            }
        } else {
            0
        };

        let result_ptr = unsafe {
            regex_find(pattern.as_ptr() as *const i8, text.as_ptr() as *const i8, group, flags)
        };

        if result_ptr.is_null() {
            Ok(Value::Nil)
        } else {
            let c_str = unsafe { std::ffi::CStr::from_ptr(result_ptr as *const c_char) };
            let result_str = c_str.to_string_lossy().into_owned();
            unsafe { free_string(result_ptr) };
            Ok(Value::String(result_str))
        }
    }

    fn builtin_regex_replace(&mut self, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 3 && args.len() != 4 {
            return Err("regex_replace expects 3-4 arguments (pattern, text, replacement, [flags])".to_string());
        }

        let pattern = match self.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("regex_replace: pattern must be a string".to_string()),
        };

        let text = match self.evaluate_expression(args[1].clone())? {
            Value::String(s) => s,
            _ => return Err("regex_replace: text must be a string".to_string()),
        };

        let replacement = match self.evaluate_expression(args[2].clone())? {
            Value::String(s) => s,
            _ => return Err("regex_replace: replacement must be a string".to_string()),
        };

        let flags = if args.len() == 4 {
            match self.evaluate_expression(args[3].clone())? {
                Value::String(s) => Self::parse_regex_flags(&s),
                _ => 0,
            }
        } else {
            0
        };

        let result_ptr = unsafe {
            regex_replace(pattern.as_ptr() as *const i8,
                          text.as_ptr() as *const i8,
                          replacement.as_ptr() as *const i8,
                          flags)
        };

        if result_ptr.is_null() {
            Err("Regex replacement failed".to_string())
        } else {
            let c_str = unsafe { std::ffi::CStr::from_ptr(result_ptr as *const c_char) };
            let result_str = c_str.to_string_lossy().into_owned();
            unsafe { free_string(result_ptr) };
            Ok(Value::String(result_str))
        }

    }

    fn builtin_regex_replace_all(&mut self, args: Vec<Expr>) -> Result<Value, String> {
        self.builtin_regex_replace(args)
    }

    fn builtin_regex_split(&mut self, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 && args.len() != 3 {
            return Err("regex_split expects 2-3 arguments (pattern, text, [flags])".to_string());
        }

        let pattern = match self.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("regex_split: pattern must be a string".to_string()),
        };

        let text = match self.evaluate_expression(args[1].clone())? {
            Value::String(s) => s,
            _ => return Err("regex_split: text must be a string".to_string()),
        };

        let flags = if args.len() == 3 {
            match self.evaluate_expression(args[2].clone())? {
                Value::String(s) => Self::parse_regex_flags(&s),
                _ => 0,
            }
        } else {
            0
        };

        let mut count: i32 = 0;
        let results_ptr = unsafe {
            regex_split(pattern.as_ptr() as *const i8,
                        text.as_ptr() as *const i8,
                        flags, &mut count)
        };

        if results_ptr.is_null() {
            return Ok(Value::Array(Vec::new()));
        }

        let mut arr = Vec::new();
        for i in 0..count {
            let c_str = unsafe {
                let ptr = *results_ptr.offset(i as isize);
                std::ffi::CStr::from_ptr(ptr as *const c_char)
            };
            arr.push(Value::String(c_str.to_string_lossy().into_owned()));
        }

        unsafe { free_split_results(results_ptr, count) };
        Ok(Value::Array(arr))
    }

    fn builtin_regex_escape(&mut self, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("regex_escape expects exactly 1 argument (string)".to_string());
        }

        let text = match self.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("regex_escape: argument must be a string".to_string()),
        };

        let special_chars = ['.', '^', '$', '*', '+', '?', '(', ')', '[', ']', '{', '}', '\\', '|'];
        let mut escaped = String::with_capacity(text.len() * 2);

        for c in text.chars() {
            if special_chars.contains(&c) {
                escaped.push('\\');
            }
            escaped.push(c);
        }

        Ok(Value::String(escaped))
    }

    fn parse_regex_flags(flags_str: &str) -> i32 {
        let mut flags = 0;
        for c in flags_str.chars() {
            match c {
                'i' => flags |= REG_ICASE,
                'm' => flags |= REG_NEWLINE,
                'e' => flags |= REG_EXTENDED,
                _ => {}
            }
        }
        flags
    }

    fn escape_regex(text: &str) -> String {
        let special_chars = ['.', '^', '$', '*', '+', '?', '(', ')', '[', ']', '{', '}', '\\', '|'];
        let mut result = String::with_capacity(text.len() * 2);

        for c in text.chars() {
            if special_chars.contains(&c) {
                result.push('\\');
            }
            result.push(c);
        }

        result
    }

    fn import_module(&mut self, filename: &str) -> Result<Rc<RefCell<Environment>>, String> {
        let resolved_path = if filename.starts_with('.') {

            let current_dir = std::env::current_dir()
                .map_err(|e| format!("Failed to get current directory: {}", e))?;

            let name = if filename.starts_with("./") {
                &filename[2..]
            } else if filename.starts_with('.') {
                &filename[1..]
            } else {
                filename
            };

            let file_path = current_dir.join(format!("{}.prism", name));
            if file_path.exists() {
                file_path.to_string_lossy().to_string()
            } else {
                let file_path_no_ext = current_dir.join(name);
                if file_path_no_ext.exists() {
                    file_path_no_ext.to_string_lossy().to_string()
                } else {
                    return Err(format!("Relative import '.{}' not found in current directory", name));
                }
            }
        } else if filename.starts_with('/') {
            let path = Path::new(filename);
            if path.exists() && path.is_file() {
                filename.to_string()
            } else {
                let with_ext = format!("{}.prism", filename);
                if Path::new(&with_ext).exists() {
                    with_ext
                } else {
                    return Err(format!("Absolute path '{}' not found", filename));
                }
            }
        } else if !filename.contains('/') && !filename.contains('\\') && !filename.contains('.') {
            let home = match std::env::var("HOME") {
                Ok(h) => h,
                Err(_) => {
                    match std::env::var("USERPROFILE") {
                        Ok(p) => p,
                        Err(_) => ".".to_string()
                    }
                }
            };

            let prism_libs = Path::new(&home).join("prism-libs");
            let package_path = prism_libs
                .join(filename)
                .join("src")
                .join(format!("{}.prism", filename));

            if package_path.exists() {
                package_path.to_string_lossy().to_string()
            } else {
                let index_path = prism_libs.join(filename).join("index.json");
                if index_path.exists() {
                    match std::fs::read_to_string(&index_path) {
                        Ok(content) => {
                            if let Some(main_file) = self.extract_main_from_index(&content) {
                                let full_main_path = prism_libs.join(filename).join(main_file);
                                if full_main_path.exists() {
                                    full_main_path.to_string_lossy().to_string()
                                } else {
                                    return Err(format!("Package '{}' main file not found", filename));
                                }
                            } else {
                                return Err(format!("Package '{}' index.json has no 'main' field", filename));
                            }
                        }
                        Err(_) => {
                            return Err(format!("Package '{}' not found in prism-libs", filename));
                        }
                    }
                } else {
                    return Err(format!("Package '{}' not found in prism-libs", filename));
                }
            }
        } else {
            let current_dir = std::env::current_dir()
                .map_err(|e| format!("Failed to get current directory: {}", e))?;

            let path = current_dir.join(filename);
            if path.exists() && path.is_file() {
                path.to_string_lossy().to_string()
            } else {
                let with_ext = format!("{}.prism", path.display());
                if Path::new(&with_ext).exists() {
                    with_ext
                } else {
                    return Err(format!("File '{}' not found", filename));
                }
            }
        };

        if self.loading_modules.contains(&resolved_path) {
            return Err(format!("Circular import detected: {}", filename));
        }

        if self.loaded_modules.contains_key(&resolved_path) {
            let env = self.loaded_modules.get(&resolved_path).unwrap().clone();
            return Ok(env);
        }

        self.loading_modules.insert(resolved_path.clone());

        let content = std::fs::read_to_string(&resolved_path)
            .map_err(|e| format!("Failed to read module '{}': {}", filename, e))?;

        let module_name = Path::new(&resolved_path)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        self.module_contents.insert(module_name.clone(), content.clone());

        let module_env = Rc::new(RefCell::new(Environment::new()));

        let old_env = std::mem::replace(&mut self.environment, module_env.clone());

        let lexer = Lexer::new(&content);
        let mut parser = Parser::new(lexer);
        let ast = parser.parse_program()
            .map_err(|e| format!("Failed to parse module '{}': {}", filename, e))?;

        let old_return = self.return_value.take();
        let old_break = self.should_break;
        let old_continue = self.should_continue;

        for stmt in ast {
            self.execute_statement(stmt)?;
            if self.return_value.is_some() {
                self.return_value = None;
            }
            if self.should_break || self.should_continue {
                self.should_break = false;
                self.should_continue = false;
            }
        }

        self.return_value = old_return;
        self.should_break = old_break;
        self.should_continue = old_continue;
        self.environment = old_env;

        self.loaded_modules.insert(resolved_path.clone(), module_env.clone());
        self.loading_modules.remove(&resolved_path);

        Ok(module_env)
    }

    fn extract_main_from_index(&self, content: &str) -> Option<String> {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("\"main\"") || trimmed.starts_with("main") {
                if let Some(colon_pos) = trimmed.find(':') {
                    let after_colon = trimmed[colon_pos+1..].trim();
                    let main_value = after_colon.trim_matches('"').trim_matches(',').trim();
                    if !main_value.is_empty() {
                        return Some(main_value.to_string());
                    }
                }
            }
        }
        None
    }

    fn builtin_strlen(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("strlen expects exactly one argument".to_string());
        }

        let value = interpreter.evaluate_expression(args[0].clone())?;

        match value {
            Value::String(s) => {
                let len = s.len();
                Ok(Value::Number(len as f64))
            }
            Value::Char(_c) => {
                Ok(Value::Number(1.0))
            }
            Value::GUIFrame(_) | Value::GUIWidget(_) => Ok(Value::Number(12.0)),
            Value::Number(n) => {
                let s = if n == n.floor() {
                    format!("{:.0}", n)
                } else {
                    n.to_string()
                };
                Ok(Value::Number(s.len() as f64))
            }
            Value::Struct(name, fields) => {
                let s = format!("{} {{ {} }}", name, fields.len());
                Ok(Value::Number(s.len() as f64))
            }
            Value::Boolean(b) => {
                let s = b.to_string();
                Ok(Value::Number(s.len() as f64))
            }
            Value::Array(arr) => {
                Ok(Value::Number(arr.len() as f64))
            }
            Value::Nil => {
                Ok(Value::Number(3.0))
            }
            Value::Function(_) => {
                Ok(Value::Number(10.0))
            }
           Value::DatabaseHandle(_) => Ok(Value::Number(12.0)),
            Value::HashMap(map) => {
                Ok(Value::Number(map.len() as f64))
            }
            Value::Enum(_, _) => {
                Ok(Value::Number(6.0))
            }
            Value::FileHandle(_) => {
                Ok(Value::Number(6.0))
            }
        }
    }

    fn builtin_strcmp(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("strcmp expects exactly 2 arguments (string1, string2)".to_string());
        }

        let c_a = std::ffi::CString::new(
            match interpreter.evaluate_expression(args[0].clone())? {
                Value::String(s) => s,
                _ => return Err("strcmp: first argument must be a string".to_string()),
            }
        ).map_err(|e| format!("strcmp: invalid UTF-8 string: {}", e))?;

        let c_b = std::ffi::CString::new(
            match interpreter.evaluate_expression(args[1].clone())? {
                Value::String(s) => s,
                _ => return Err("strcmp: second argument must be a string".to_string()),
            }
        ).map_err(|e| format!("strcmp: invalid UTF-8 string: {}", e))?;

        let result = unsafe {
            strcmp_prism(c_a.as_ptr() as *const i8, c_b.as_ptr() as *const i8)
        };

        Ok(Value::Number(result as f64))
    }
    fn builtin_str_contains(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("str_contains expects exactly 2 arguments (haystack, needle)".to_string());
        }

        let haystack = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_contains: first argument must be a string".to_string()),
        };

        let needle = match interpreter.evaluate_expression(args[1].clone())? {
            Value::String(s) => s,
            _ => return Err("str_contains: second argument must be a string".to_string()),
        };

        Ok(Value::Boolean(haystack.contains(&needle)))
    }

    fn builtin_str_replace(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 3 {
            return Err("str_replace expects exactly 3 arguments (search, replace, subject)".to_string());
        }

        let search = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_replace: first argument must be a string".to_string()),
        };

        let replace = match interpreter.evaluate_expression(args[1].clone())? {
            Value::String(s) => s,
            _ => return Err("str_replace: second argument must be a string".to_string()),
        };

        let subject = match interpreter.evaluate_expression(args[2].clone())? {
            Value::String(s) => s,
            _ => return Err("str_replace: third argument must be a string".to_string()),
        };

        Ok(Value::String(subject.replace(&search, &replace)))
    }

    fn builtin_str_split(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("str_split expects exactly 1 argument (string)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_split: argument must be a string".to_string()),
        };

        let mut chars = Vec::new();
        for ch in text.chars() {
            chars.push(Value::String(ch.to_string()));
        }

        Ok(Value::Array(chars))
    }

    fn builtin_trim(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("trim expects exactly 1 argument (string)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("trim: argument must be a string".to_string()),
        };

        Ok(Value::String(text.trim().to_string()))
    }

    fn builtin_str_repeat(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("str_repeat expects exactly 2 arguments (string, times)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_repeat: first argument must be a string".to_string()),
        };

        let times = match interpreter.evaluate_expression(args[1].clone())? {
            Value::Number(n) => n as usize,
            _ => return Err("str_repeat: second argument must be a number".to_string()),
        };

        if times == 0 {
            return Ok(Value::String("".to_string()));
        }

        Ok(Value::String(text.repeat(times)))
    }

    fn builtin_strpos(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("strpos expects exactly 2 arguments (haystack, needle)".to_string());
        }

        let haystack = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("strpos: first argument must be a string".to_string()),
        };

        let needle = match interpreter.evaluate_expression(args[1].clone())? {
            Value::String(s) => s,
            _ => return Err("strpos: second argument must be a string".to_string()),
        };

        match haystack.find(&needle) {
            Some(pos) => Ok(Value::Number(pos as f64)),
            None => Ok(Value::Number(-1.0)),
        }
    }

    fn builtin_strtoupper(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("strtoupper expects exactly 1 argument (string)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("strtoupper: argument must be a string".to_string()),
        };

        Ok(Value::String(text.to_uppercase()))
    }

    fn builtin_strtolower(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("strtolower expects exactly 1 argument (string)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("strtolower: argument must be a string".to_string()),
        };

        Ok(Value::String(text.to_lowercase()))
    }

    fn builtin_str_pad(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() < 3 || args.len() > 4 {
            return Err("str_pad expects 3-4 arguments (string, length, pad_string, [type])".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_pad: first argument must be a string".to_string()),
        };

        let length = match interpreter.evaluate_expression(args[1].clone())? {
            Value::Number(n) => n as usize,
            _ => return Err("str_pad: second argument must be a number".to_string()),
        };

        let pad = match interpreter.evaluate_expression(args[2].clone())? {
            Value::String(s) => s,
            _ => return Err("str_pad: third argument must be a string".to_string()),
        };

        let pad_type = if args.len() == 4 {
            match interpreter.evaluate_expression(args[3].clone())? {
                Value::Number(n) => n as i32,
                _ => 2,
            }
        } else {
            2
        };

        if text.len() >= length {
            return Ok(Value::String(text));
        }

        let total_pad_needed = length - text.len();
        let pad_len = pad.len();
        let mut result = String::new();

        let generate_pad = |count: usize| -> String {
            if count == 0 { return String::new(); }
            let mut p = String::new();
            while p.len() < count {
                p = p + &pad;
            }
            p.truncate(count);
            p
        };

        match pad_type {
            0 => {
                let left_pad = total_pad_needed / 2;
                let right_pad = total_pad_needed - left_pad;
                result.push_str(&generate_pad(left_pad));
                result.push_str(&text);
                result.push_str(&generate_pad(right_pad));
            }
            1 => {
                result.push_str(&generate_pad(total_pad_needed));
                result.push_str(&text);
            }
            2 => {
                result.push_str(&text);
                result.push_str(&generate_pad(total_pad_needed));
            }
            _ => {
                result.push_str(&text);
                result.push_str(&generate_pad(total_pad_needed));
            }
        }

        Ok(Value::String(result))
    }

    fn builtin_str_reverse(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("str_reverse expects exactly 1 argument (string)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_reverse: argument must be a string".to_string()),
        };

        Ok(Value::String(text.chars().rev().collect()))
    }

    fn builtin_str_shuffle(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("str_shuffle expects exactly 1 argument (string)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_shuffle: argument must be a string".to_string()),
        };

        let mut chars: Vec<char> = text.chars().collect();

        for i in (1..chars.len()).rev() {
            let random = Self::builtin_rand_float(interpreter, Vec::new())?;
            let j = match random {
                Value::Number(n) => (n * (i + 1) as f64) as usize,
                _ => i,
            };
            let idx = if j > i { i } else { j };
            chars.swap(i, idx);
        }

        Ok(Value::String(chars.into_iter().collect()))
    }

    fn builtin_str_word_count(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("str_word_count expects exactly 1 argument (string)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_word_count: argument must be a string".to_string()),
        };

        let mut count = 0;
        let mut in_word = false;

        for c in text.chars() {
            if c.is_alphabetic() || c == '\'' || c == '-' {
                if !in_word {
                    count += 1;
                    in_word = true;
                }
            } else {
                in_word = false;
            }
        }

        Ok(Value::Number(count as f64))
    }

    fn builtin_str_ends_with(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("str_ends_with expects exactly 2 arguments (haystack, needle)".to_string());
        }

        let haystack = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_ends_with: first argument must be a string".to_string()),
        };

        let needle = match interpreter.evaluate_expression(args[1].clone())? {
            Value::String(s) => s,
            _ => return Err("str_ends_with: second argument must be a string".to_string()),
        };

        Ok(Value::Boolean(haystack.ends_with(&needle)))
    }

    fn builtin_str_starts_with(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("str_starts_with expects exactly 2 arguments (haystack, needle)".to_string());
        }

        let haystack = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_starts_with: first argument must be a string".to_string()),
        };

        let needle = match interpreter.evaluate_expression(args[1].clone())? {
            Value::String(s) => s,
            _ => return Err("str_starts_with: second argument must be a string".to_string()),
        };

        Ok(Value::Boolean(haystack.starts_with(&needle)))
    }

    fn builtin_html_escape(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("html_escape expects exactly 1 argument (string)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("html_escape: argument must be a string".to_string()),
        };

        let mut result = String::with_capacity(text.len() * 2);
        for c in text.chars() {
            match c {
                '&' => result.push_str("&amp;"),
                '<' => result.push_str("&lt;"),
                '>' => result.push_str("&gt;"),
                '"' => result.push_str("&quot;"),
                '\'' => result.push_str("&#039;"),
                _ => result.push(c),
            }
        }

        Ok(Value::String(result))
    }

    fn builtin_html_unescape(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("html_unescape expects exactly 1 argument (string)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("html_unescape: argument must be a string".to_string()),
        };

        let mut result = text;
        result = result.replace("&amp;", "&");
        result = result.replace("&lt;", "<");
        result = result.replace("&gt;", ">");
        result = result.replace("&quot;", "\"");
        result = result.replace("&#039;", "'");
        result = result.replace("&#39;", "'");

        Ok(Value::String(result))
    }

    fn builtin_str_trim_left(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("str_trim_left expects exactly 1 argument (string)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_trim_left: argument must be a string".to_string()),
        };

        Ok(Value::String(text.trim_start().to_string()))
    }

    fn builtin_str_trim_right(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("str_trim_right expects exactly 1 argument (string)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_trim_right: argument must be a string".to_string()),
        };

        Ok(Value::String(text.trim_end().to_string()))
    }

    fn builtin_str_swapcase(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("str_swapcase expects exactly 1 argument (string)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_swapcase: argument must be a string".to_string()),
        };

        let mut result = String::with_capacity(text.len());
        for c in text.chars() {
            if c.is_lowercase() {
                result.push(c.to_uppercase().next().unwrap_or(c));
            } else if c.is_uppercase() {
                result.push(c.to_lowercase().next().unwrap_or(c));
            } else {
                result.push(c);
            }
        }

        Ok(Value::String(result))
    }

    fn builtin_str_capitalize(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("str_capitalize expects exactly 1 argument (string)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_capitalize: argument must be a string".to_string()),
        };

        if text.is_empty() {
            return Ok(Value::String(text));
        }

        let mut chars = text.chars();
        let first = chars.next().unwrap();
        let rest: String = chars.collect();
        Ok(Value::String(format!("{}{}", first.to_uppercase(), rest)))
    }

    fn builtin_str_title(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("str_title expects exactly 1 argument (string)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_title: argument must be a string".to_string()),
        };

        let mut result = String::new();
        let mut capitalize = true;

        for c in text.chars() {
            if c.is_whitespace() {
                capitalize = true;
                result.push(c);
            } else if capitalize {
                result.push(c.to_uppercase().next().unwrap_or(c));
                capitalize = false;
            } else {
                result.push(c.to_lowercase().next().unwrap_or(c));
            }
        }

        Ok(Value::String(result))
    }

    fn builtin_str_snake_case(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("str_snake_case expects exactly 1 argument (string)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_snake_case: argument must be a string".to_string()),
        };

        let mut result = String::new();
        let mut prev_was_upper = false;
        let mut first = true;

        for c in text.chars() {
            if c.is_uppercase() {
                if !first && !prev_was_upper {
                    result.push('_');
                }
                result.push(c.to_lowercase().next().unwrap_or(c));
                prev_was_upper = true;
            } else if c == ' ' || c == '-' || c == '_' {
                result.push('_');
                prev_was_upper = false;
            } else {
                result.push(c);
                prev_was_upper = false;
            }
            first = false;
        }

        Ok(Value::String(result))
    }

    fn builtin_str_camel_case(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("str_camel_case expects exactly 1 argument (string)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_camel_case: argument must be a string".to_string()),
        };

        let mut result = String::new();
        let mut next_upper = false;
        let mut first = true;

        for c in text.chars() {
            if c == ' ' || c == '-' || c == '_' {
                next_upper = true;
            } else if next_upper {
                result.push(c.to_uppercase().next().unwrap_or(c));
                next_upper = false;
            } else if first {
                result.push(c.to_lowercase().next().unwrap_or(c));
                first = false;
            } else {
                result.push(c);
            }
        }

        Ok(Value::String(result))
    }

    fn builtin_str_kebab_case(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("str_kebab_case expects exactly 1 argument (string)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_kebab_case: argument must be a string".to_string()),
        };

        let mut result = String::new();
        let mut prev_was_upper = false;
        let mut first = true;

        for c in text.chars() {
            if c.is_uppercase() {
                if !first && !prev_was_upper {
                    result.push('-');
                }
                result.push(c.to_lowercase().next().unwrap_or(c));
                prev_was_upper = true;
            } else if c == ' ' || c == '_' {
                result.push('-');
                prev_was_upper = false;
            } else {
                result.push(c);
                prev_was_upper = false;
            }
            first = false;
        }

        let mut cleaned = String::new();
        let mut prev_dash = false;
        for c in result.chars() {
            if c == '-' {
                if !prev_dash {
                    cleaned.push(c);
                    prev_dash = true;
                }
            } else {
                cleaned.push(c);
                prev_dash = false;
            }
        }
        let trimmed = cleaned.trim_matches('-');

        Ok(Value::String(trimmed.to_string()))
    }

    fn builtin_str_after(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("str_after expects exactly 2 arguments (string, substring)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_after: first argument must be a string".to_string()),
        };

        let search = match interpreter.evaluate_expression(args[1].clone())? {
            Value::String(s) => s,
            _ => return Err("str_after: second argument must be a string".to_string()),
        };

        if let Some(pos) = text.find(&search) {
            let after = pos + search.len();
            if after < text.len() {
                Ok(Value::String(text[after..].to_string()))
            } else {
                Ok(Value::String("".to_string()))
            }
        } else {
            Ok(Value::String(text))
        }
    }

    fn builtin_str_before(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("str_before expects exactly 2 arguments (string, substring)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_before: first argument must be a string".to_string()),
        };

        let search = match interpreter.evaluate_expression(args[1].clone())? {
            Value::String(s) => s,
            _ => return Err("str_before: second argument must be a string".to_string()),
        };

        if let Some(pos) = text.find(&search) {
            Ok(Value::String(text[..pos].to_string()))
        } else {
            Ok(Value::String(text))
        }
    }

    fn builtin_str_after_last(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("str_after_last expects exactly 2 arguments (string, substring)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_after_last: first argument must be a string".to_string()),
        };

        let search = match interpreter.evaluate_expression(args[1].clone())? {
            Value::String(s) => s,
            _ => return Err("str_after_last: second argument must be a string".to_string()),
        };

        if let Some(pos) = text.rfind(&search) {
            let after = pos + search.len();
            if after < text.len() {
                Ok(Value::String(text[after..].to_string()))
            } else {
                Ok(Value::String("".to_string()))
            }
        } else {
            Ok(Value::String(text))
        }
    }

    fn builtin_str_before_last(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("str_before_last expects exactly 2 arguments (string, substring)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_before_last: first argument must be a string".to_string()),
        };

        let search = match interpreter.evaluate_expression(args[1].clone())? {
            Value::String(s) => s,
            _ => return Err("str_before_last: second argument must be a string".to_string()),
        };

        if let Some(pos) = text.rfind(&search) {
            Ok(Value::String(text[..pos].to_string()))
        } else {
            Ok(Value::String(text))
        }
    }

    fn builtin_str_is_empty(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("str_is_empty expects exactly 1 argument (string)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_is_empty: argument must be a string".to_string()),
        };

        Ok(Value::Boolean(text.is_empty()))
    }

    fn builtin_str_is_blank(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("str_is_blank expects exactly 1 argument (string)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_is_blank: argument must be a string".to_string()),
        };

        Ok(Value::Boolean(text.trim().is_empty()))
    }

    fn builtin_str_is_numeric(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("str_is_numeric expects exactly 1 argument (string)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_is_numeric: argument must be a string".to_string()),
        };

        Ok(Value::Boolean(text.chars().all(|c| c.is_ascii_digit())))
    }

    fn builtin_str_is_alpha(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("str_is_alpha expects exactly 1 argument (string)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_is_alpha: argument must be a string".to_string()),
        };

        Ok(Value::Boolean(text.chars().all(|c| c.is_alphabetic())))
    }

    fn builtin_str_is_alphanumeric(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("str_is_alphanumeric expects exactly 1 argument (string)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_is_alphanumeric: argument must be a string".to_string()),
        };

        Ok(Value::Boolean(text.chars().all(|c| c.is_alphanumeric())))
    }

    fn builtin_str_is_lowercase(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("str_is_lowercase expects exactly 1 argument (string)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_is_lowercase: argument must be a string".to_string()),
        };

        if text.is_empty() {
            return Ok(Value::Boolean(true));
        }

        Ok(Value::Boolean(text.chars().all(|c| !c.is_alphabetic() || c.is_lowercase())))
    }

    fn builtin_str_is_uppercase(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("str_is_uppercase expects exactly 1 argument (string)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_is_uppercase: argument must be a string".to_string()),
        };

        if text.is_empty() {
            return Ok(Value::Boolean(true));
        }

        Ok(Value::Boolean(text.chars().all(|c| !c.is_alphabetic() || c.is_uppercase())))
    }

    fn builtin_str_truncate(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("str_truncate expects exactly 2 arguments (string, length)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_truncate: first argument must be a string".to_string()),
        };

        let length = match interpreter.evaluate_expression(args[1].clone())? {
            Value::Number(n) => n as usize,
            _ => return Err("str_truncate: second argument must be a number".to_string()),
        };

        if text.len() <= length {
            return Ok(Value::String(text));
        }

        if length <= 3 {
            return Ok(Value::String("...".to_string()));
        }

        Ok(Value::String(format!("{}...", &text[..length - 3])))
    }

    fn builtin_str_truncate_middle(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("str_truncate_middle expects exactly 2 arguments (string, length)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_truncate_middle: first argument must be a string".to_string()),
        };

        let length = match interpreter.evaluate_expression(args[1].clone())? {
            Value::Number(n) => n as usize,
            _ => return Err("str_truncate_middle: second argument must be a number".to_string()),
        };

        if text.len() <= length {
            return Ok(Value::String(text));
        }

        if length <= 3 {
            return Ok(Value::String("...".to_string()));
        }

        let half = (length - 3) / 2;
        let left = &text[..half];
        let right = &text[text.len() - half..];

        Ok(Value::String(format!("{}...{}", left, right)))
    }

    fn builtin_str_reverse_words(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("str_reverse_words expects exactly 1 argument (string)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_reverse_words: argument must be a string".to_string()),
        };

        let words: Vec<&str> = text.split_whitespace().collect();
        let reversed: Vec<&str> = words.into_iter().rev().collect();

        Ok(Value::String(reversed.join(" ")))
    }

    fn builtin_str_word_wrap(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("str_word_wrap expects exactly 2 arguments (string, width)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_word_wrap: first argument must be a string".to_string()),
        };

        let width = match interpreter.evaluate_expression(args[1].clone())? {
            Value::Number(n) => n as usize,
            _ => return Err("str_word_wrap: second argument must be a number".to_string()),
        };

        if width == 0 {
            return Ok(Value::String(text));
        }

        let mut result = String::new();
        let mut line = String::new();

        for word in text.split_whitespace() {
            if line.len() + word.len() + 1 > width && !line.is_empty() {
                result.push_str(&line);
                result.push('\n');
                line.clear();
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }

        if !line.is_empty() {
            result.push_str(&line);
        }

        Ok(Value::String(result))
    }

    fn builtin_str_remove(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("str_remove expects exactly 2 arguments (string, substring)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_remove: first argument must be a string".to_string()),
        };

        let search = match interpreter.evaluate_expression(args[1].clone())? {
            Value::String(s) => s,
            _ => return Err("str_remove: second argument must be a string".to_string()),
        };

        if let Some(pos) = text.find(&search) {
            let (left, right) = text.split_at(pos);
            let (_, after) = right.split_at(search.len());
            Ok(Value::String(format!("{}{}", left, after)))
        } else {
            Ok(Value::String(text))
        }
    }

    fn builtin_str_remove_all(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("str_remove_all expects exactly 2 arguments (string, substring)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("str_remove_all: first argument must be a string".to_string()),
        };

        let search = match interpreter.evaluate_expression(args[1].clone())? {
            Value::String(s) => s,
            _ => return Err("str_remove_all: second argument must be a string".to_string()),
        };

        Ok(Value::String(text.replace(&search, "")))
    }
    fn builtin_getenv(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("getenv expects exactly 1 argument (variable name)".to_string());
        }

        let var_name = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("getenv: argument must be a string".to_string()),
        };

        match std::env::var(&var_name) {
            Ok(value) => Ok(Value::String(value)),
            Err(_) => Ok(Value::Nil),
        }
    }

    fn builtin_setenv(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("setenv expects exactly 2 arguments (variable name, value)".to_string());
        }

        let var_name = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("setenv: first argument must be a string".to_string()),
        };

        let var_value = match interpreter.evaluate_expression(args[1].clone())? {
            Value::String(s) => s,
            Value::Number(n) => n.to_string(),
            Value::Boolean(b) => if b { "true".to_string() } else { "false".to_string() },
            Value::Nil => "nil".to_string(),
            _ => return Err("setenv: second argument must be a string, number, or boolean".to_string()),
        };

        std::env::set_var(&var_name, &var_value);
        Ok(Value::Nil)
    }

    fn builtin_read_dir(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("read_dir expects exactly 1 argument (path)".to_string());
        }

        let path = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("read_dir: argument must be a string".to_string()),
        };

        let entries = match std::fs::read_dir(&path) {
            Ok(entries) => entries,
            Err(e) => return Err(format!("Failed to read directory '{}': {}", path, e)),
        };

        let mut files = Vec::new();
        for entry in entries {
            match entry {
                Ok(entry) => {
                    let file_name = entry.file_name();
                    if let Some(name) = file_name.to_str() {
                        files.push(Value::String(name.to_string()));
                    }
                }
                Err(e) => {
                    continue;
                }
            }
        }

        Ok(Value::Array(files))
    }
  fn builtin_csv_write(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("csv_write expects exactly 2 arguments (filename, data)".to_string());
        }

        let filename = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("csv_write: first argument must be a string".to_string()),
        };

        let data = match interpreter.evaluate_expression(args[1].clone())? {
            Value::Array(arr) => arr,
            _ => return Err("csv_write: second argument must be an array".to_string()),
        };

        let mut csv_lines = Vec::new();
        for row in data {
            let row_data = match row {
                Value::Array(row_arr) => row_arr,
                _ => return Err("csv_write: each row must be an array".to_string()),
            };

            let mut csv_row = Vec::new();
            for cell in row_data {
                let cell_str = match cell {
                    Value::String(s) => format!("\"{}\"", s.replace("\"", "\"\"")),
                    Value::Number(n) => n.to_string(),
                    Value::Boolean(b) => if b { "true".to_string() } else { "false".to_string() },
                    Value::Nil => "".to_string(),
                    _ => cell.to_string(),
                };
                csv_row.push(cell_str);
            }
            csv_lines.push(csv_row.join(","));
        }

        let csv_content = csv_lines.join("\n");

        use std::io::Write;
        match std::fs::File::create(&filename) {
            Ok(mut file) => {
                match file.write_all(csv_content.as_bytes()) {
                    Ok(_) => Ok(Value::Boolean(true)),
                    Err(e) => Err(format!("Failed to write CSV: {}", e)),
                }
            }
            Err(e) => Err(format!("Failed to create file: {}", e)),
        }
    }

    fn builtin_csv_read(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("csv_read expects exactly 1 argument (filename)".to_string());
        }

        let filename = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("csv_read: first argument must be a string".to_string()),
        };

        let content = match std::fs::read_to_string(&filename) {
            Ok(s) => s,
            Err(e) => return Err(format!("Failed to read file: {}", e)),
        };

        let mut result = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        for line in lines {
            if line.trim().is_empty() {
                continue;
            }

            let mut row = Vec::new();
            let mut current = String::new();
            let mut in_quotes = false;
            let chars: Vec<char> = line.chars().collect();
            let mut i = 0;

            while i < chars.len() {
                let c = chars[i];

                if c == '"' {
                    if in_quotes && i + 1 < chars.len() && chars[i + 1] == '"' {
                        current.push('"');
                        i += 2;
                        continue;
                    }
                    in_quotes = !in_quotes;
                    i += 1;
                    continue;
                }

                if c == ',' && !in_quotes {
                    row.push(Value::String(current.trim().to_string()));
                    current.clear();
                    i += 1;
                    continue;
                }

                current.push(c);
                i += 1;
            }

            if !current.is_empty() || !row.is_empty() {
                row.push(Value::String(current.trim().to_string()));
            }

            if !row.is_empty() {
                result.push(Value::Array(row));
            }
        }

        Ok(Value::Array(result))
    }

    fn builtin_csv_to_json(&mut self, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("csv_to_json expects exactly 1 argument (file_path)".to_string());
        }

        let file_path = match self.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("csv_to_json: argument must be a string (file path)".to_string()),
        };

        let content = std::fs::read_to_string(&file_path)
            .map_err(|e| format!("Failed to read CSV file '{}': {}", file_path, e))?;

        let mut lines = content.lines().peekable();
        if lines.peek().is_none() {
            return Ok(Value::String("[]".to_string()));
        }

        let header_line = lines.next().unwrap();
        let headers: Vec<String> = header_line
            .split(',')
            .map(|s| s.trim().trim_matches('"').to_string())
            .collect();

        if headers.is_empty() {
            return Ok(Value::String("[]".to_string()));
        }

        let mut json_array = Vec::new();

        for line in lines {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let mut fields: Vec<String> = Vec::new();
            let mut current_field = String::new();
            let mut in_quotes = false;

            for c in line.chars() {
                match c {
                    '"' => in_quotes = !in_quotes,
                    ',' => {
                        if in_quotes {
                            current_field.push(c);
                        } else {
                            fields.push(current_field.trim().to_string());
                            current_field.clear();
                        }
                    }
                    _ => current_field.push(c),
                }
            }
            if !current_field.is_empty() || !fields.is_empty() {
                fields.push(current_field.trim().to_string());
            }

            while fields.len() < headers.len() {
                fields.push("".to_string());
            }

            let mut json_object = String::new();
            json_object.push('{');

            for (i, header) in headers.iter().enumerate() {
                let value = if i < fields.len() {
                    fields[i].trim().to_string()
                } else {
                    "".to_string()
                };

                let json_value = if value.is_empty() {
                    "null".to_string()
                } else if value == "true" || value == "false" {
                    value.to_string()
                } else if let Ok(num) = value.parse::<f64>() {
                    if num == num.floor() {
                        format!("{:.0}", num)
                    } else {
                        value.to_string()
                    }
                } else {
                    format!("\"{}\"", value.replace('"', "\\\""))
                };

                json_object.push('"');
                json_object.push_str(&header.replace('"', "\\\""));
                json_object.push('"');
                json_object.push(':');
                json_object.push_str(&json_value);

                if i < headers.len() - 1 {
                    json_object.push(',');
                }
            }

            json_object.push('}');
            json_array.push(json_object);
        }

        let mut result = String::from("[\n");
        for (i, obj) in json_array.iter().enumerate() {
            if i > 0 {
                result.push(',');
            }
            result.push_str("    ");
            result.push_str(obj);
            result.push('\n');
        }
        result.push(']');

        Ok(Value::String(result))
    }

    fn get_file(&self, handle: usize) -> Result<&std::fs::File, String> {
        if handle >= self.files.len() {
            return Err("Invalid file handle".to_string());
        }
        match &self.files[handle] {
            Some(file) => Ok(file),
            None => Err("File handle is closed".to_string()),
        }
    }

    fn get_file_mut(&mut self, handle: usize) -> Result<&mut std::fs::File, String> {
        if handle >= self.files.len() {
            return Err("Invalid file handle".to_string());
        }
        match &mut self.files[handle] {
            Some(file) => Ok(file),
            None => Err("File handle is closed".to_string()),
        }
    }

    fn builtin_fopen(&mut self, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("fopen expects exactly 2 arguments (filename, mode)".to_string());
        }

        let filename = match self.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("fopen: filename must be a string".to_string()),
        };

        let mode = match self.evaluate_expression(args[1].clone())? {
            Value::String(s) => s,
            _ => return Err("fopen: mode must be a string".to_string()),
        };

        let file = match mode.as_str() {
            "r" => std::fs::File::open(&filename).ok(),
            "w" => std::fs::File::create(&filename).ok(),
            "a" => std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(&filename)
                .ok(),
            "r+" => std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&filename)
                .ok(),
            "w+" => std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(&filename)
                .ok(),
            "a+" => std::fs::OpenOptions::new()
                .read(true)
                .append(true)
                .create(true)
                .open(&filename)
                .ok(),
            _ => return Err(format!("Unsupported file mode: {}", mode)),
        };

        match file {
            Some(f) => {
                let handle = self.files.len();
                self.files.push(Some(f));
                Ok(Value::FileHandle(handle))
            }
            None => Ok(Value::Nil),
        }
    }

    fn builtin_fclose(&mut self, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("fclose expects exactly 1 argument (file handle)".to_string());
        }

        let handle = match self.evaluate_expression(args[0].clone())? {
            Value::FileHandle(h) => h,
            _ => return Err("fclose: argument must be a file handle".to_string()),
        };

        if handle < self.files.len() {
            self.files[handle] = None;
            Ok(Value::Nil)
        } else {
            Err("Invalid file handle".to_string())
        }
    }

    fn builtin_fwrite(&mut self, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("fwrite expects exactly 2 arguments (file handle, string)".to_string());
        }

        let handle = match self.evaluate_expression(args[0].clone())? {
            Value::FileHandle(h) => h,
            _ => return Err("fwrite: first argument must be a file handle".to_string()),
        };

        let content = match self.evaluate_expression(args[1].clone())? {
            Value::String(s) => s,
            _ => return Err("fwrite: second argument must be a string".to_string()),
        };

        let file = self.get_file_mut(handle)?;
        use std::io::Write;
        match file.write_all(content.as_bytes()) {
            Ok(_) => Ok(Value::Number(content.len() as f64)),
            Err(e) => Err(format!("Failed to write to file: {}", e)),
        }
    }

    fn builtin_fwrite_line(&mut self, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("fwrite_line expects exactly 2 arguments (file handle, string)".to_string());
        }

        let handle = match self.evaluate_expression(args[0].clone())? {
            Value::FileHandle(h) => h,
            _ => return Err("fwrite_line: first argument must be a file handle".to_string()),
        };

        let content = match self.evaluate_expression(args[1].clone())? {
            Value::String(s) => s,
            _ => return Err("fwrite_line: second argument must be a string".to_string()),
        };

        let file = self.get_file_mut(handle)?;
        use std::io::Write;
        let line = content + "\n";
        match file.write_all(line.as_bytes()) {
            Ok(_) => Ok(Value::Number(line.len() as f64)),
            Err(e) => Err(format!("Failed to write line to file: {}", e)),
        }
    }

    fn builtin_fread(&mut self, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("fread expects exactly 1 argument (file handle)".to_string());
        }

        let handle = match self.evaluate_expression(args[0].clone())? {
            Value::FileHandle(h) => h,
            _ => return Err("fread: argument must be a file handle".to_string()),
        };

        let file = self.get_file(handle)?;
        use std::io::Read;
        let mut reader = std::io::BufReader::new(file);
        let mut content = String::new();

        match reader.read_to_string(&mut content) {
            Ok(_) => Ok(Value::String(content)),
            Err(e) => Err(format!("Failed to read file: {}", e)),
        }
    }

    fn builtin_fread_line(&mut self, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("fread_line expects exactly 1 argument (file handle)".to_string());
        }

        let handle = match self.evaluate_expression(args[0].clone())? {
            Value::FileHandle(h) => h,
            _ => return Err("fread_line: argument must be a file handle".to_string()),
        };

        let file = self.get_file(handle)?;
        use std::io::BufRead;
        let mut reader = std::io::BufReader::new(file);
        let mut line = String::new();

        match reader.read_line(&mut line) {
            Ok(0) => Ok(Value::Nil),
            Ok(_) => {
                if line.ends_with('\n') {
                    line.pop();
                    if line.ends_with('\r') {
                        line.pop();
                    }
                }
                Ok(Value::String(line))
            }
            Err(e) => Err(format!("Failed to read line: {}", e)),
        }
    }

    fn builtin_fread_lines(&mut self, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("fread_lines expects exactly 1 argument (file handle)".to_string());
        }

        let handle = match self.evaluate_expression(args[0].clone())? {
            Value::FileHandle(h) => h,
            _ => return Err("fread_lines: argument must be a file handle".to_string()),
        };

        let file = self.get_file(handle)?;
        use std::io::BufRead;
        let reader = std::io::BufReader::new(file);
        let lines: Vec<Value> = reader
            .lines()
            .filter_map(Result::ok)
            .map(Value::String)
            .collect();

        Ok(Value::Array(lines))
    }

    fn builtin_ftell(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("ftell expects exactly 1 argument (file_handle)".to_string());
        }

        let handle = match interpreter.evaluate_expression(args[0].clone())? {
            Value::FileHandle(h) => h,
            _ => return Err("ftell: argument must be a file handle".to_string()),
        };

        let mut file = interpreter.get_file_mut(handle)?;
        use std::io::Seek;

        match file.stream_position() {
            Ok(pos) => Ok(Value::Number(pos as f64)),
            Err(e) => Err(format!("ftell failed: {}", e)),
        }
    }

    fn builtin_fseek(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 3 {
            return Err("fseek expects exactly 3 arguments (file_handle, offset, whence)".to_string());
        }

        let handle = match interpreter.evaluate_expression(args[0].clone())? {
            Value::FileHandle(h) => h,
            _ => return Err("fseek: first argument must be a file handle".to_string()),
        };

        let offset = match interpreter.evaluate_expression(args[1].clone())? {
            Value::Number(n) => n as i64,
            _ => return Err("fseek: second argument must be a number".to_string()),
        };

        let whence = match interpreter.evaluate_expression(args[2].clone())? {
            Value::Number(n) => n as i32,
            _ => return Err("fseek: third argument must be a number (0=SEEK_SET, 1=SEEK_CUR, 2=SEEK_END)".to_string()),
        };

        let mut file = interpreter.get_file_mut(handle)?;
        use std::io::Seek;
        use std::io::SeekFrom;

        let seek_from = match whence {
            0 => SeekFrom::Start(offset as u64),
            1 => SeekFrom::Current(offset),
            2 => SeekFrom::End(offset),
            _ => return Err("fseek: whence must be 0 (SEEK_SET), 1 (SEEK_CUR), or 2 (SEEK_END)".to_string()),
        };

        match file.seek(seek_from) {
            Ok(_) => Ok(Value::Number(0.0)),
            Err(e) => Err(format!("fseek failed: {}", e)),
        }
    }

    fn builtin_feof(&mut self, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("feof expects exactly 1 argument (file handle)".to_string());
        }

        let handle = match self.evaluate_expression(args[0].clone())? {
            Value::FileHandle(h) => h,
            _ => return Err("feof: argument must be a file handle".to_string()),
        };

        let file = self.get_file(handle)?;
        let mut reader = std::io::BufReader::new(file);
        let mut buffer = [0; 1];

        match reader.read_exact(&mut buffer) {
            Ok(_) => Ok(Value::Boolean(false)),
            Err(_) => Ok(Value::Boolean(true)),
        }
    }

    fn builtin_rewind(&mut self, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("rewind expects exactly 1 argument (file handle)".to_string());
        }

        let handle = match self.evaluate_expression(args[0].clone())? {
            Value::FileHandle(h) => h,
            _ => return Err("rewind: argument must be a file handle".to_string()),
        };

        let file = self.get_file_mut(handle)?;
        use std::io::Seek;

        match file.rewind() {
            Ok(_) => Ok(Value::Nil),
            Err(e) => Err(format!("Failed to rewind: {}", e)),
        }
    }

    fn builtin_file_remove(&mut self, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("file_remove expects exactly 1 argument (filename)".to_string());
        }

        let filename = match self.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("file_remove: filename must be a string".to_string()),
        };

        match std::fs::remove_file(&filename) {
            Ok(_) => Ok(Value::Boolean(true)),
            Err(_) => Ok(Value::Boolean(false)),
        }
    }

    fn builtin_file_rename(&mut self, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("file_rename expects exactly 2 arguments (oldname, newname)".to_string());
        }

        let oldname = match self.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("file_rename: oldname must be a string".to_string()),
        };

        let newname = match self.evaluate_expression(args[1].clone())? {
            Value::String(s) => s,
            _ => return Err("file_rename: newname must be a string".to_string()),
        };

        match std::fs::rename(&oldname, &newname) {
            Ok(_) => Ok(Value::Boolean(true)),
            Err(_) => Ok(Value::Boolean(false)),
        }
    }

    fn builtin_file_exists(&mut self, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("file_exists expects exactly 1 argument (filename)".to_string());
        }

        let filename = match self.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("file_exists: filename must be a string".to_string()),
        };

        Ok(Value::Boolean(std::path::Path::new(&filename).exists()))
    }

    fn builtin_fflush(&mut self, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("fflush expects exactly 1 argument (file handle)".to_string());
        }

        let handle = match self.evaluate_expression(args[0].clone())? {
            Value::FileHandle(h) => h,
            _ => return Err("fflush: argument must be a file handle".to_string()),
        };

        let file = self.get_file(handle)?;
        use std::io::Write;
        let mut writer = std::io::BufWriter::new(file);
        match writer.flush() {
            Ok(_) => Ok(Value::Nil),
            Err(e) => Err(format!("Failed to flush: {}", e)),
        }
    }

    fn builtin_file_copy(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("file_copy expects exactly 2 arguments (source, destination)".to_string());
        }

        let source = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("file_copy: first argument must be a string".to_string()),
        };

        let dest = match interpreter.evaluate_expression(args[1].clone())? {
            Value::String(s) => s,
            _ => return Err("file_copy: second argument must be a string".to_string()),
        };

        match std::fs::copy(&source, &dest) {
            Ok(_) => Ok(Value::Boolean(true)),
            Err(e) => Err(format!("Failed to copy file: {}", e)),
        }
    }

    fn builtin_file_move(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("file_move expects exactly 2 arguments (source, destination)".to_string());
        }

        let source = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("file_move: first argument must be a string".to_string()),
        };

        let dest = match interpreter.evaluate_expression(args[1].clone())? {
            Value::String(s) => s,
            _ => return Err("file_move: second argument must be a string".to_string()),
        };

        match std::fs::rename(&source, &dest) {
            Ok(_) => Ok(Value::Boolean(true)),
            Err(e) => Err(format!("Failed to move file: {}", e)),
        }
    }

    fn builtin_file_size(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("file_size expects exactly 1 argument (filename)".to_string());
        }

        let filename = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("file_size: argument must be a string".to_string()),
        };

        match std::fs::metadata(&filename) {
            Ok(metadata) => Ok(Value::Number(metadata.len() as f64)),
            Err(e) => Err(format!("Failed to get file size: {}", e)),
        }
    }

    fn builtin_file_modified(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("file_modified expects exactly 1 argument (filename)".to_string());
        }

        let filename = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("file_modified: argument must be a string".to_string()),
        };

        match std::fs::metadata(&filename) {
            Ok(metadata) => {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(timestamp) = modified.duration_since(std::time::UNIX_EPOCH) {
                        return Ok(Value::Number(timestamp.as_secs() as f64));
                    }
                }
                Ok(Value::Nil)
            }
            Err(e) => Err(format!("Failed to get file modified time: {}", e)),
        }
    }

    fn builtin_file_created(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("file_created expects exactly 1 argument (filename)".to_string());
        }

        let filename = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("file_created: argument must be a string".to_string()),
        };

        match std::fs::metadata(&filename) {
            Ok(metadata) => {
                #[cfg(target_os = "linux")]
                {
                    use std::os::linux::fs::MetadataExt;
                    let created = metadata.st_ctime();
                    return Ok(Value::Number(created as f64));
                }
                #[cfg(target_os = "macos")]
                {
                    use std::os::macos::fs::MetadataExt;
                    let created = metadata.st_ctime();
                    return Ok(Value::Number(created as f64));
                }
                #[cfg(target_os = "windows")]
                {
                    use std::os::windows::fs::MetadataExt;
                    let created = metadata.creation_time();
                    return Ok(Value::Number(created as f64));
                }
                #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
                {
                    Ok(Value::Nil)
                }
            }
            Err(e) => Err(format!("Failed to get file creation time: {}", e)),
        }
    }

    fn builtin_file_is_dir(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("file_is_dir expects exactly 1 argument (path)".to_string());
        }

        let path = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("file_is_dir: argument must be a string".to_string()),
        };

        match std::fs::metadata(&path) {
            Ok(metadata) => Ok(Value::Boolean(metadata.is_dir())),
            Err(_) => Ok(Value::Boolean(false)),
        }
    }

    fn builtin_file_is_file(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("file_is_file expects exactly 1 argument (path)".to_string());
        }

        let path = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("file_is_file: argument must be a string".to_string()),
        };

        match std::fs::metadata(&path) {
            Ok(metadata) => Ok(Value::Boolean(metadata.is_file())),
            Err(_) => Ok(Value::Boolean(false)),
        }
    }

    fn builtin_mkdir(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("mkdir expects exactly 1 argument (path)".to_string());
        }

        let path = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("mkdir: argument must be a string".to_string()),
        };

        match std::fs::create_dir_all(&path) {
            Ok(_) => Ok(Value::Boolean(true)),
            Err(e) => Err(format!("Failed to create directory: {}", e)),
        }
    }

    fn builtin_rmdir(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("rmdir expects exactly 1 argument (path)".to_string());
        }

        let path = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("rmdir: argument must be a string".to_string()),
        };

        match std::fs::remove_dir(&path) {
            Ok(_) => Ok(Value::Boolean(true)),
            Err(e) => Err(format!("Failed to remove directory: {}", e)),
        }
    }

    fn builtin_file_append(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("file_append expects exactly 2 arguments (filename, content)".to_string());
        }

        let filename = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("file_append: first argument must be a string".to_string()),
        };

        let content = match interpreter.evaluate_expression(args[1].clone())? {
            Value::String(s) => s,
            _ => return Err("file_append: second argument must be a string".to_string()),
        };

        use std::io::Write;
        match std::fs::OpenOptions::new().append(true).create(true).open(&filename) {
            Ok(mut file) => {
                match file.write_all(content.as_bytes()) {
                    Ok(_) => Ok(Value::Number(content.len() as f64)),
                    Err(e) => Err(format!("Failed to append to file: {}", e)),
                }
            }
            Err(e) => Err(format!("Failed to open file for append: {}", e)),
        }
    }

    fn builtin_file_write_lines(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("file_write_lines expects exactly 2 arguments (filename, lines)".to_string());
        }

        let filename = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("file_write_lines: first argument must be a string".to_string()),
        };

        let lines = match interpreter.evaluate_expression(args[1].clone())? {
            Value::Array(arr) => arr,
            _ => return Err("file_write_lines: second argument must be an array".to_string()),
        };

        use std::io::Write;
        match std::fs::File::create(&filename) {
            Ok(mut file) => {
                for line in &lines {
                    let line_str = line.to_string();
                    if let Err(e) = file.write_all((line_str + "\n").as_bytes()) {
                        return Err(format!("Failed to write line: {}", e));
                    }
                }
                Ok(Value::Number(lines.len() as f64))
            }
            Err(e) => Err(format!("Failed to create file: {}", e)),
        }
    }

    fn builtin_is_enum(&mut self, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("is_enum expects exactly 1 argument".to_string());
        }

        let value = self.evaluate_expression(args[0].clone())?;
        Ok(Value::Boolean(matches!(value, Value::Enum(_, _))))
    }

    fn builtin_enum_name(&mut self, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("enum_name expects exactly 1 argument".to_string());
        }

        let value = self.evaluate_expression(args[0].clone())?;
        match value {
            Value::Enum(name, _) => Ok(Value::String(name)),
            _ => Err("Value is not an enum".to_string()),
        }
    }

    fn builtin_enum_variant(&mut self, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("enum_variant expects exactly 1 argument".to_string());
        }

        let value = self.evaluate_expression(args[0].clone())?;
        match value {
            Value::Enum(_, variant) => Ok(Value::String(variant)),
            _ => Err("Value is not an enum".to_string()),
        }
    }

    fn get_map(&self, name: &str) -> Result<HashMap<Value, Value>, String> {
        let value = self.environment.borrow().get(name)
            .ok_or_else(|| format!("Variable '{}' not found", name))?;

        match value {
            Value::HashMap(map) => Ok(map),
            _ => Err(format!("Variable '{}' is not a map", name)),
        }
    }

    fn set_map(&mut self, name: &str, map: HashMap<Value, Value>) -> Result<(), String> {
        self.environment.borrow_mut().set(name.to_string(), Value::HashMap(map))
    }

    fn builtin_map_get_value(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("map_get_value expects exactly 2 arguments (map, key)".to_string());
        }

        let map_name = match &args[0] {
            Expr::Variable(name) => name.clone(),
            _ => return Err("map_get_value: first argument must be a map variable".to_string()),
        };

        let key = interpreter.evaluate_expression(args[1].clone())?;

        let map = interpreter.get_map(&map_name)?;

        match map.get(&key) {
            Some(value) => Ok(value.clone()),
            None => Ok(Value::Nil),
        }
    }

    fn builtin_map_push(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 3 {
            return Err("map_push expects exactly 3 arguments (map, key, value)".to_string());
        }

        let map_name = match &args[0] {
            Expr::Variable(name) => name.clone(),
            _ => return Err("map_push: first argument must be a map variable".to_string()),
        };

        let key = interpreter.evaluate_expression(args[1].clone())?;
        let value = interpreter.evaluate_expression(args[2].clone())?;

        let mut map = interpreter.get_map(&map_name)?;
        map.insert(key, value);
        interpreter.set_map(&map_name, map)?;

        Ok(Value::Number(0.0))
    }

    fn builtin_map_peek(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("map_peek expects exactly 1 argument (map)".to_string());
        }

        let map_name = match &args[0] {
            Expr::Variable(name) => name.clone(),
            _ => return Err("map_peek: argument must be a map variable".to_string()),
        };

        let map = interpreter.get_map(&map_name)?;

        if let Some((key, value)) = map.iter().next() {
            Ok(Value::Array(vec![key.clone(), value.clone()]))
        } else {
            Ok(Value::Nil)
        }
    }
    fn builtin_map_get_index(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("map_get_index expects exactly 2 arguments (map, key)".to_string());
        }

        let map_name = match &args[0] {
            Expr::Variable(name) => name.clone(),
            _ => return Err("map_get_index: first argument must be a map variable".to_string()),
        };

        let search_key = interpreter.evaluate_expression(args[1].clone())?;

        let map = interpreter.get_map(&map_name)?;

        let mut index = 0;
        for (key, _) in map.iter() {
            if key == &search_key {
                return Ok(Value::Number(index as f64));
            }
            index += 1;
        }

        Ok(Value::Number(-1.0))
    }

    fn builtin_map_remove(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("map_remove expects exactly 2 arguments (map, key)".to_string());
        }

        let map_name = match &args[0] {
            Expr::Variable(name) => name.clone(),
            _ => return Err("map_remove: first argument must be a map variable".to_string()),
        };

        let key = interpreter.evaluate_expression(args[1].clone())?;

        let mut map = interpreter.get_map(&map_name)?;

        if map.remove(&key).is_some() {
            interpreter.set_map(&map_name, map)?;
            Ok(Value::Number(0.0))
        } else {
            Ok(Value::Number(1.0))
        }
    }

    fn builtin_map_sort_as_key(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("map_sort_as_key expects exactly 1 argument (map)".to_string());
        }

        let map_name = match &args[0] {
            Expr::Variable(name) => name.clone(),
            _ => return Err("map_sort_as_key: argument must be a map variable".to_string()),
        };

        let map = interpreter.get_map(&map_name)?;

        let mut sorted: Vec<(Value, Value)> = map.into_iter().collect();
        sorted.sort_by(|a, b| {
            let a_key = a.0.to_string();
            let b_key = b.0.to_string();
            a_key.cmp(&b_key)
        });

        let result: Vec<Value> = sorted.into_iter()
            .map(|(k, v)| Value::Array(vec![k, v]))
            .collect();

        Ok(Value::Array(result))
    }

    fn builtin_map_sort_as_value(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("map_sort_as_value expects exactly 1 argument (map)".to_string());
        }

        let map_name = match &args[0] {
            Expr::Variable(name) => name.clone(),
            _ => return Err("map_sort_as_value: argument must be a map variable".to_string()),
        };

        let map = interpreter.get_map(&map_name)?;

        let mut sorted: Vec<(Value, Value)> = map.into_iter().collect();
        sorted.sort_by(|a, b| {
            let a_val = a.1.to_string();
            let b_val = b.1.to_string();
            a_val.cmp(&b_val)
        });

        let result: Vec<Value> = sorted.into_iter()
            .map(|(k, v)| Value::Array(vec![k, v]))
            .collect();

        Ok(Value::Array(result))
    }

    fn builtin_map_len(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("map_len expects exactly 1 argument (map)".to_string());
        }

        let map_name = match &args[0] {
            Expr::Variable(name) => name.clone(),
            _ => return Err("map_len: argument must be a map variable".to_string()),
        };

        let map = interpreter.get_map(&map_name)?;
        Ok(Value::Number(map.len() as f64))
    }

    fn builtin_map_is_key_exists(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("map_is_key_exists expects exactly 2 arguments (map, key)".to_string());
        }

        let map_name = match &args[0] {
            Expr::Variable(name) => name.clone(),
            _ => return Err("map_is_key_exists: first argument must be a map variable".to_string()),
        };

        let key = interpreter.evaluate_expression(args[1].clone())?;

        let map = interpreter.get_map(&map_name)?;
        Ok(Value::Boolean(map.contains_key(&key)))
    }

    fn builtin_map_is_value_exists(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("map_is_value_exists expects exactly 2 arguments (map, value)".to_string());
        }

        let map_name = match &args[0] {
            Expr::Variable(name) => name.clone(),
            _ => return Err("map_is_value_exists: first argument must be a map variable".to_string()),
        };

        let search_value = interpreter.evaluate_expression(args[1].clone())?;

        let map = interpreter.get_map(&map_name)?;

        for value in map.values() {
            if value == &search_value {
                return Ok(Value::Boolean(true));
            }
        }

        Ok(Value::Boolean(false))
    }
    fn builtin_map_get_key(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("map_get_key expects exactly 2 arguments (map, value)".to_string());
        }

        let map_name = match &args[0] {
            Expr::Variable(name) => name.clone(),
            _ => return Err("map_get_key: first argument must be a map variable".to_string()),
        };

        let search_value = interpreter.evaluate_expression(args[1].clone())?;

        let map = interpreter.get_map(&map_name)?;

        for (key, value) in map.iter() {
            if value == &search_value {
                return Ok(key.clone());
            }
        }

        Ok(Value::Nil)
    }
    fn builtin_map_keys(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("map_keys expects exactly 1 argument (map)".to_string());
        }

        let map_name = match &args[0] {
            Expr::Variable(name) => name.clone(),
            _ => return Err("map_keys: argument must be a map variable".to_string()),
        };

        let map = interpreter.get_map(&map_name)?;
        let keys: Vec<Value> = map.keys().cloned().collect();
        Ok(Value::Array(keys))
    }

    fn builtin_map_values(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("map_values expects exactly 1 argument (map)".to_string());
        }

        let map_name = match &args[0] {
            Expr::Variable(name) => name.clone(),
            _ => return Err("map_values: argument must be a map variable".to_string()),
        };

        let map = interpreter.get_map(&map_name)?;
        let values: Vec<Value> = map.values().cloned().collect();
        Ok(Value::Array(values))
    }

    fn builtin_map_clear(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("map_clear expects exactly 1 argument (map)".to_string());
        }

        let map_name = match &args[0] {
            Expr::Variable(name) => name.clone(),
            _ => return Err("map_clear: argument must be a map variable".to_string()),
        };

        interpreter.set_map(&map_name, HashMap::new())?;
        Ok(Value::Nil)
    }

    fn builtin_map_copy(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("map_copy expects exactly 1 argument (map)".to_string());
        }

        let map_name = match &args[0] {
            Expr::Variable(name) => name.clone(),
            _ => return Err("map_copy: argument must be a map variable".to_string()),
        };

        let map = interpreter.get_map(&map_name)?;
        Ok(Value::HashMap(map.clone()))
    }

    fn builtin_map_merge(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("map_merge expects exactly 2 arguments (map1, map2)".to_string());
        }

        let map1_name = match &args[0] {
            Expr::Variable(name) => name.clone(),
            _ => return Err("map_merge: first argument must be a map variable".to_string()),
        };

        let map2_name = match &args[1] {
            Expr::Variable(name) => name.clone(),
            _ => return Err("map_merge: second argument must be a map variable".to_string()),
        };

        let mut map1 = interpreter.get_map(&map1_name)?;
        let map2 = interpreter.get_map(&map2_name)?;

        for (k, v) in map2 {
            map1.insert(k, v);
        }
        Ok(Value::HashMap(map1))
    }

    fn builtin_var_dump(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if !args.is_empty() {
            return Err("var_dump expects no arguments".to_string());
        }

        let mut result = HashMap::new();
        let mut seen = std::collections::HashSet::new();

        let mut current_env = interpreter.environment.clone();

        loop {
            let env_ref = current_env.clone();
            let env = env_ref.borrow();

            for (name, value) in env.values.iter() {
                if !seen.contains(name) {
                    seen.insert(name.clone());
                    let display_name = format!("${}", name);
                    result.insert(Value::String(display_name), value.clone());
                }
            }

            if let Some(parent) = env.parent.clone() {
                drop(env);
                current_env = parent;
            } else {
                break;
            }
        }

        Ok(Value::HashMap(result))
    }

    fn builtin_http_get(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("http_get expects exactly 1 argument (url)".to_string());
        }

        let url = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("http_get: url must be a string".to_string()),
        };

        let result_ptr = unsafe {
            http_get(url.as_ptr() as *const i8)
        };

        if result_ptr.is_null() {
            return Err("HTTP request failed".to_string());
        }

        let c_str = unsafe { std::ffi::CStr::from_ptr(result_ptr as *const c_char) };
        let result_str = c_str.to_string_lossy().into_owned();
        unsafe { free_string(result_ptr as *const i8) };

        Ok(Value::String(result_str))
    }

    fn builtin_http_post(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("http_post expects exactly 2 arguments (url, data)".to_string());
        }

        let url = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("http_post: url must be a string".to_string()),
        };

        let data = match interpreter.evaluate_expression(args[1].clone())? {
            Value::String(s) => s,
            _ => return Err("http_post: data must be a string".to_string()),
        };

        let result_ptr = unsafe {
            http_post(url.as_ptr() as *const i8, data.as_ptr() as *const i8)
        };

        if result_ptr.is_null() {
            return Err("HTTP POST request failed".to_string());
        }

        let c_str = unsafe { std::ffi::CStr::from_ptr(result_ptr as *const c_char) };
        let result_str = c_str.to_string_lossy().into_owned();
        unsafe { free_string(result_ptr as *const i8) };

        Ok(Value::String(result_str))
    }

    fn builtin_http_request(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() < 2 || args.len() > 4 {
            return Err("http_request expects 2-4 arguments (method, url, [headers], [body])".to_string());
        }

        let method = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("http_request: method must be a string".to_string()),
        };

        let url = match interpreter.evaluate_expression(args[1].clone())? {
            Value::String(s) => s,
            _ => return Err("http_request: url must be a string".to_string()),
        };

        let headers = if args.len() >= 3 {
            match interpreter.evaluate_expression(args[2].clone())? {
                Value::String(s) => s,
                _ => "".to_string(),
            }
        } else {
            "".to_string()
        };

        let body = if args.len() >= 4 {
            match interpreter.evaluate_expression(args[3].clone())? {
                Value::String(s) => s,
                _ => "".to_string(),
            }
        } else {
            "".to_string()
        };

        let result_ptr = unsafe {
            http_request(
                method.as_ptr() as *const i8,
                url.as_ptr() as *const i8,
                headers.as_ptr() as *const i8,
                body.as_ptr() as *const i8
            )
        };

        if result_ptr.is_null() {
            return Err("HTTP request failed".to_string());
        }

        let c_str = unsafe { std::ffi::CStr::from_ptr(result_ptr as *const c_char) };
        let result_str = c_str.to_string_lossy().into_owned();
        unsafe { free_string(result_ptr as *const i8) };

        Ok(Value::String(result_str))
    }

    fn builtin_substr(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() < 2 || args.len() > 3 {
            return Err("substr expects 2-3 arguments (string, start, [length])".to_string());
        }

        let string = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("substr: first argument must be a string".to_string()),
        };

        let start_val = interpreter.evaluate_expression(args[1].clone())?;
        let start = match start_val {
            Value::Number(n) => n as i64,
            _ => return Err("substr: start must be a number".to_string()),
        };

        let len = string.len() as i64;
        let mut start_pos = if start < 0 {
            len + start
        } else {
            start
        };

        if start_pos < 0 {
            start_pos = 0;
        }
        if start_pos > len {
            start_pos = len;
        }

        let end_pos = if args.len() == 3 {
            let length_val = interpreter.evaluate_expression(args[2].clone())?;
            let length = match length_val {
                Value::Number(n) => n as i64,
                _ => return Err("substr: length must be a number".to_string()),
            };

            if length < 0 {
                len
            } else {
                let mut end = start_pos + length;
                if end > len {
                    end = len;
                }
                end
            }
        } else {
            len
        };

        let result = if start_pos < end_pos {
            &string[start_pos as usize..end_pos as usize]
        } else {
            ""
        };

        Ok(Value::String(result.to_string()))
    }

    fn builtin_sha256_string(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("sha256_string expects exactly 1 argument (string)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("sha256_string: argument must be a string".to_string()),
        };

        let hash_bytes = Sha256::digest(text.as_bytes());
        let hash_hex = hash_bytes.iter().map(|b| format!("{:02x}", b)).collect::<String>();

        Ok(Value::String(hash_hex))
    }

    fn builtin_sha256_file(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("sha256_file expects exactly 1 argument (filename)".to_string());
        }

        let filename = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("sha256_file: filename must be a string".to_string()),
        };

        use std::fs::File;
        use std::io::Read;

        let mut file = File::open(&filename)
            .map_err(|e| format!("Failed to open file: {}", e))?;

        let mut hasher = Sha256::default();
        let mut buffer = [0; 8192];

        loop {
            let bytes_read = file.read(&mut buffer)
                .map_err(|e| format!("Failed to read file: {}", e))?;

            if bytes_read == 0 {
                break;
            }

            hasher.update(&buffer[..bytes_read]);
        }

        let hash_bytes = hasher.finish();
        let hash_hex = hash_bytes.iter().map(|b| format!("{:02x}", b)).collect::<String>();

        Ok(Value::String(hash_hex))
    }

    fn builtin_sha512_string(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("sha512_string expects exactly 1 argument (string)".to_string());
        }

        let text = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("sha512_string: argument must be a string".to_string()),
        };

        let hash_bytes = Sha512::digest(text.as_bytes());
        let hash_hex = hash_bytes.iter().map(|b| format!("{:02x}", b)).collect::<String>();

        Ok(Value::String(hash_hex))
    }

    fn builtin_sha512_file(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("sha512_file expects exactly 1 argument (filename)".to_string());
        }

        let filename = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("sha512_file: filename must be a string".to_string()),
        };

        use std::fs::File;
        use std::io::Read;

        let mut file = File::open(&filename)
            .map_err(|e| format!("Failed to open file: {}", e))?;

        let mut hasher = Sha512::new();
        let mut buffer = [0; 8192];

        loop {
            let bytes_read = file.read(&mut buffer)
                .map_err(|e| format!("Failed to read file: {}", e))?;

            if bytes_read == 0 {
                break;
            }

            hasher.update(&buffer[..bytes_read]);
        }

        let hash_bytes = hasher.finalize();
        let hash_hex = hash_bytes.iter().map(|b| format!("{:02x}", b)).collect::<String>();

        Ok(Value::String(hash_hex))
    }

    // ========== SQLITE: OPEN ==========
    fn builtin_db_open(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("db_open expects exactly 1 argument (filename)".to_string());
        }
    
        let filename = match interpreter.evaluate_expression(args[0].clone())? {
            Value::String(s) => s,
            _ => return Err("db_open: filename must be a string".to_string()),
        };
    
        let c_filename = std::ffi::CString::new(filename.as_str())
            .map_err(|e| format!("Invalid filename: {}", e))?;
    
        let db_ptr = unsafe { db_open(c_filename.as_ptr() as *const i8) };
    
        if db_ptr.is_null() {
            return Err(format!("db_open: failed to open '{}'", filename));
        }
    
        let handle = interpreter.databases.len();
        interpreter.databases.push(Some(db_ptr));
    
        Ok(Value::DatabaseHandle(handle))
    }
    
    // ========== SQLITE: CLOSE ==========
    fn builtin_db_close(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("db_close expects exactly 1 argument (database)".to_string());
        }
    
        let handle = match interpreter.evaluate_expression(args[0].clone())? {
            Value::DatabaseHandle(h) => h,
            _ => return Err("db_close: argument must be a database handle".to_string()),
        };
    
        if handle >= interpreter.databases.len() {
            return Err("db_close: invalid database handle".to_string());
        }
    
        if let Some(db_ptr) = interpreter.databases[handle].take() {
            unsafe { db_close(db_ptr); }
        }
    
        Ok(Value::Nil)
    }
    
    // ========== SQLITE: EXECUTE RAW SQL ==========
    fn builtin_db_execute(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 2 {
            return Err("db_execute expects exactly 2 arguments (database, sql)".to_string());
        }
    
        let handle = match interpreter.evaluate_expression(args[0].clone())? {
            Value::DatabaseHandle(h) => h,
            _ => return Err("db_execute: first argument must be a database handle".to_string()),
        };
    
        let sql = match interpreter.evaluate_expression(args[1].clone())? {
            Value::String(s) => s,
            _ => return Err("db_execute: sql must be a string".to_string()),
        };
    
        if handle >= interpreter.databases.len() || interpreter.databases[handle].is_none() {
            return Err("db_execute: invalid or closed database handle".to_string());
        }
    
        let db_ptr = interpreter.databases[handle].unwrap();
    
        let c_sql = std::ffi::CString::new(sql.as_str())
            .map_err(|e| format!("Invalid SQL: {}", e))?;
    
        let mut err_buf = [0i8; 512];
        let result = unsafe {
            db_execute(
                db_ptr,
                c_sql.as_ptr() as *const i8,
                err_buf.as_mut_ptr(),
                512,
            )
        };
    
        if result != 0 {
            let err_msg = unsafe {
                std::ffi::CStr::from_ptr(err_buf.as_ptr() as *const c_char)
                    .to_string_lossy()
                    .into_owned()
            };
            return Err(format!("db_execute error: {}", err_msg));
        }
    
        Ok(Value::Nil)
    }
    
    // ========== SQLITE: QUERY (with params) ==========
    fn builtin_db_query(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 3 {
            return Err("db_query expects exactly 3 arguments (database, sql, params_array)".to_string());
        }
    
        let handle = match interpreter.evaluate_expression(args[0].clone())? {
            Value::DatabaseHandle(h) => h,
            _ => return Err("db_query: first argument must be a database handle".to_string()),
        };
    
        let sql = match interpreter.evaluate_expression(args[1].clone())? {
            Value::String(s) => s,
            _ => return Err("db_query: sql must be a string".to_string()),
        };
    
        // Convert params array to C strings
        let params_val = interpreter.evaluate_expression(args[2].clone())?;
        let param_strings: Vec<String> = match params_val {
            Value::Array(arr) => arr.iter().map(|v| v.to_string()).collect(),
            _ => return Err("db_query: third argument must be an array of parameters".to_string()),
        };
    
        if handle >= interpreter.databases.len() || interpreter.databases[handle].is_none() {
            return Err("db_query: invalid or closed database handle".to_string());
        }
    
        let db_ptr = interpreter.databases[handle].unwrap();
    
        let c_sql = std::ffi::CString::new(sql.as_str())
            .map_err(|e| format!("Invalid SQL: {}", e))?;
    
        // Build array of C strings for params
        let c_params: Vec<std::ffi::CString> = param_strings.iter()
            .map(|s| std::ffi::CString::new(s.as_str()).unwrap_or_else(|_| std::ffi::CString::new("").unwrap()))
            .collect();
    
        let mut param_ptrs: Vec<*const i8> = c_params.iter()
            .map(|c| c.as_ptr() as *const i8)
            .collect();
    
        let result_ptr = unsafe {
            db_query(
                db_ptr,
                c_sql.as_ptr() as *const i8,
                param_ptrs.as_mut_ptr(),
                c_params.len() as i32,
            )
        };
    
        if result_ptr.is_null() {
            let err_msg = unsafe {
                let err_cstr = db_error(db_ptr);
                std::ffi::CStr::from_ptr(err_cstr as *const c_char).to_string_lossy().into_owned()
            };
            return Err(format!("db_query error: {}", err_msg));
        }
    
        let result_str = unsafe {
            let s = std::ffi::CStr::from_ptr(result_ptr as *const c_char).to_string_lossy().into_owned();
            db_free_result(result_ptr);
            s
        };
    
        Ok(Value::String(result_str))
    }
    
    // ========== SQLITE: EXECUTE WITH PARAMS (INSERT/UPDATE/DELETE) ==========
    fn builtin_db_execute_params(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 3 {
            return Err("db_execute_params expects exactly 3 arguments (database, sql, params_array)".to_string());
        }
    
        let handle = match interpreter.evaluate_expression(args[0].clone())? {
            Value::DatabaseHandle(h) => h,
            _ => return Err("db_execute_params: first argument must be a database handle".to_string()),
        };
    
        let sql = match interpreter.evaluate_expression(args[1].clone())? {
            Value::String(s) => s,
            _ => return Err("db_execute_params: sql must be a string".to_string()),
        };
    
        let params_val = interpreter.evaluate_expression(args[2].clone())?;
        let param_strings: Vec<String> = match params_val {
            Value::Array(arr) => arr.iter().map(|v| v.to_string()).collect(),
            _ => return Err("db_execute_params: third argument must be an array of parameters".to_string()),
        };
    
        if handle >= interpreter.databases.len() || interpreter.databases[handle].is_none() {
            return Err("db_execute_params: invalid or closed database handle".to_string());
        }
    
        let db_ptr = interpreter.databases[handle].unwrap();
    
        let c_sql = std::ffi::CString::new(sql.as_str())
            .map_err(|e| format!("Invalid SQL: {}", e))?;
    
        let c_params: Vec<std::ffi::CString> = param_strings.iter()
            .map(|s| std::ffi::CString::new(s.as_str()).unwrap_or_else(|_| std::ffi::CString::new("").unwrap()))
            .collect();
    
        let mut param_ptrs: Vec<*const i8> = c_params.iter()
            .map(|c| c.as_ptr() as *const i8)
            .collect();
    
        let affected = unsafe {
            db_execute_params(
                db_ptr,
                c_sql.as_ptr() as *const i8,
                param_ptrs.as_mut_ptr(),
                c_params.len() as i32,
            )
        };
    
        if affected < 0 {
            return Err("db_execute_params: query failed".to_string());
        }
    
        Ok(Value::Number(affected as f64))
    }
    
    // ========== SQLITE: LAST INSERT ID ==========
    fn builtin_db_last_insert_id(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("db_last_insert_id expects exactly 1 argument (database)".to_string());
        }
    
        let handle = match interpreter.evaluate_expression(args[0].clone())? {
            Value::DatabaseHandle(h) => h,
            _ => return Err("db_last_insert_id: argument must be a database handle".to_string()),
        };
    
        if handle >= interpreter.databases.len() || interpreter.databases[handle].is_none() {
            return Err("db_last_insert_id: invalid or closed database handle".to_string());
        }
    
        let db_ptr = interpreter.databases[handle].unwrap();
        let id = unsafe { db_last_insert_id(db_ptr) };
    
        Ok(Value::Number(id as f64))
    }
    
    // ========== SQLITE: ERROR MESSAGE ==========
    fn builtin_db_error(interpreter: &mut Interpreter, args: Vec<Expr>) -> Result<Value, String> {
        if args.len() != 1 {
            return Err("db_error expects exactly 1 argument (database)".to_string());
        }
    
        let handle = match interpreter.evaluate_expression(args[0].clone())? {
            Value::DatabaseHandle(h) => h,
            _ => return Err("db_error: argument must be a database handle".to_string()),
        };
    
        if handle >= interpreter.databases.len() || interpreter.databases[handle].is_none() {
            return Err("db_error: invalid or closed database handle".to_string());
        }
    
        let db_ptr = interpreter.databases[handle].unwrap();
        let err_msg = unsafe {
            let err_cstr = db_error(db_ptr);
            std::ffi::CStr::from_ptr(err_cstr as *const c_char).to_string_lossy().into_owned()
        };
    
        Ok(Value::String(err_msg))
    }

    fn module_to_string(&self, module_name: &str) -> Result<Value, String> {
        let actual_module = self.module_aliases.get(module_name)
            .unwrap_or(&module_name.to_string())
            .clone();

        if self.aliased_modules.contains(&actual_module) && !self.alias_to_module.contains_key(module_name) {
            return Err(format!(
                "Module '{}' was imported with an alias. Use the alias instead.",
                actual_module
            ));
        }

        if !self.module_contents.contains_key(&actual_module) {
            return Err(format!("Module '{}' not found", module_name));
        }

        let source = self.module_contents.get(&actual_module).unwrap();

        let hash = self.hash_string(source);

        Ok(Value::String(format!(
            "Module: {} ({} chars, hash: {})",
            actual_module,
            source.len(),
            hash
        )))
    }

    fn hash_string(&self, s: &str) -> String {
        let mut hash: u64 = 0;

        for (i, ch) in s.chars().enumerate() {
            let byte = ch as u64;
            hash = hash.wrapping_add(byte);
            hash = hash.wrapping_mul(31);
            hash ^= (byte << (i % 8)) | (byte >> (8 - (i % 8)));
            hash = hash.rotate_left(7);
            hash ^= hash >> 33;
            hash = hash.wrapping_mul(0xff51afd7ed558ccd);
            hash ^= hash >> 33;
            hash = hash.wrapping_mul(0xc4ceb9fe1a85ec53);
            hash ^= hash >> 33;
        }

        format!("{:016x}", hash)
    }

    fn execute_statement(&mut self, stmt: Statement) -> Result<(), String> {
        match stmt {
            Statement::Expr(expr) => {
                self.evaluate_expression(expr)?;
                Ok(())
            }
            Statement::Let(name, expr) => {
                let value = self.evaluate_expression(expr)?;
                self.environment.borrow_mut().define(name, value);
                Ok(())
            }
            Statement::MultipleLet(variables, expressions) => {
                let mut values = Vec::new();
                for expr in expressions {
                    values.push(self.evaluate_expression(expr)?);
                }

                for (var, value) in variables.iter().zip(values.iter()) {
                    self.environment.borrow_mut().define(var.clone(), value.clone());
                }
                Ok(())
            }
            Statement::Import(filename, alias) => {
                let module_env = self.import_module(&filename)?;
                let module_key = if let Some(alias_name) = alias {
                    alias_name
                } else {
                    Path::new(&filename)
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string()
                };
                if !self.loaded_modules.contains_key(&module_key) {
                    self.loaded_modules.insert(module_key, module_env);
                }
                Ok(())
            }
            Statement::Assign(name, expr) => {
                let value = self.evaluate_expression(expr)?;
                self.environment.borrow_mut().assign(name, value)
            }
            Statement::If(condition, then_branch, else_if_branches, else_branch) => {
                let cond_value = self.evaluate_expression(condition)?;
                if self.is_truthy(&cond_value) {
                    self.execute_block(then_branch)?;
                    if self.should_break || self.should_continue {
                        return Ok(());
                    }
                } else {
                    let mut executed = false;
                    for (else_if_cond, else_if_body) in else_if_branches {
                        let else_if_value = self.evaluate_expression(else_if_cond)?;
                        if self.is_truthy(&else_if_value) {
                            self.execute_block(else_if_body)?;
                            if self.should_break || self.should_continue {
                                return Ok(());
                            }
                            executed = true;
                            break;
                        }
                    }
                    if !executed {
                        if let Some(else_body) = else_branch {
                            self.execute_block(else_body)?;
                            if self.should_break || self.should_continue {
                                return Ok(());
                            }
                        }
                    }
                }
                Ok(())
            }
            Statement::While(condition, body) => {
                while {
                    if self.should_break {
                        self.should_break = false;
                        return Ok(());
                    }
                    if self.should_continue {
                        self.should_continue = false;
                    }

                    let cond_value = self.evaluate_expression(condition.clone())?;
                    self.is_truthy(&cond_value)
                } {
                    self.execute_block(body.clone())?;

                    if self.should_break {
                        self.should_break = false;
                        break;
                    }

                    if self.should_continue {
                        self.should_continue = false;
                        continue;
                    }

                    if self.return_value.is_some() {
                        break;
                    }
                }
                Ok(())
            }
            Statement::Foreach(iterable, variables, body) => {
                let value = self.evaluate_expression(iterable)?;

                match value {
                    Value::Array(arr) => {
                        if variables.len() == 1 {
                            let var_name = &variables[0];
                            for item in arr {
                                self.environment.borrow_mut().define(var_name.clone(), item);
                                self.execute_block(body.clone())?;
                                if self.should_break || self.should_continue || self.return_value.is_some() {
                                    break;
                                }
                            }
                        } else if variables.len() == 2 {
                            return Err(format!(
                                "Array iteration with 2 variables not supported.\n\
                                 Hint: Use 1 variable for array values, e.g., 'foreach $arr will $x'\n\
                                 For indexed iteration, use a 'for' loop instead."
                            ));
                        } else {
                            return Err(format!(
                                "Array iteration requires exactly 1 variable, got {}.\n\
                                 Hint: Use 'foreach $arr will $x' to iterate over array values.",
                                variables.len()
                            ));
                        }
                    }

                    Value::Number(n) => {
                        if n.fract() != 0.0 {
                            return Err(format!(
                                "Cannot iterate over float: {}.\n\
                                 Hint: Only integers are supported for number iteration.",
                                n
                            ));
                        }

                        if n < 0.0 {
                            return Err(format!(
                                "Cannot iterate over negative number: {}.\n\
                                 Hint: Only non-negative integers are supported.",
                                n
                            ));
                        }

                        if variables.len() == 1 {
                            let var_name = &variables[0];
                            let num_str = n.to_string();
                            for ch in num_str.chars() {
                                if let Some(digit) = ch.to_digit(10) {
                                    self.environment.borrow_mut().define(var_name.clone(), Value::Number(digit as f64));
                                    self.execute_block(body.clone())?;
                                    if self.should_break || self.should_continue || self.return_value.is_some() {
                                        break;
                                    }
                                }
                            }
                        } else if variables.len() == 2 {
                            return Err(format!(
                                "Number iteration with 2 variables not supported.\n\
                                 Hint: Use 1 variable for digits, e.g., 'foreach 123 will $digit'"
                            ));
                        } else {
                            return Err(format!(
                                "Number iteration requires exactly 1 variable, got {}.\n\
                                 Hint: Use 'foreach $num will $digit' to iterate over digits.",
                                variables.len()
                            ));
                        }
                    }

                    Value::String(s) => {
                        if variables.len() == 1 {
                            let var_name = &variables[0];
                            for ch in s.chars() {
                                self.environment.borrow_mut().define(var_name.clone(), Value::Char(ch));
                                self.execute_block(body.clone())?;
                                if self.should_break || self.should_continue || self.return_value.is_some() {
                                    break;
                                }
                            }
                        } else if variables.len() == 2 {
                            return Err(format!(
                                "String iteration with 2 variables not supported.\n\
                                 Hint: Use 1 variable for characters, e.g., 'foreach \"hello\" will $ch'"
                            ));
                        } else {
                            return Err(format!(
                                "String iteration requires exactly 1 variable, got {}.\n\
                                 Hint: Use 'foreach \"text\" will $ch' to iterate over characters.",
                                variables.len()
                            ));
                        }
                    }

                    Value::HashMap(map) => {
                        if variables.len() == 0 {
                            return Err(format!(
                                "Map iteration requires 2 variables (key, value), got 0.\n\
                                 Hint: Use 'foreach $map will $key, $value'"
                            ));
                        } else if variables.len() == 1 {
                            return Err(format!(
                                "Map iteration requires exactly 2 variables (key and value), got 1.\n\
                                 Hint: Use 'foreach $map will $key, $value' to iterate over map entries."
                            ));
                        } else if variables.len() == 2 {
                            let key_var = &variables[0];
                            let val_var = &variables[1];
                            for (key, val) in map {
                                self.environment.borrow_mut().define(key_var.clone(), key);
                                self.environment.borrow_mut().define(val_var.clone(), val);
                                self.execute_block(body.clone())?;
                                if self.should_break || self.should_continue || self.return_value.is_some() {
                                    break;
                                }
                            }
                        } else {
                            return Err(format!(
                                "Map iteration requires exactly 2 variables (key and value), got {}.\n\
                                 Hint: Use 'foreach $map will $key, $value'",
                                variables.len()
                            ));
                        }
                    }

                    _ => {
                        let type_name = match value {
                            Value::Number(_) => "number",
                            Value::String(_) => "string",
                            Value::Array(_) => "array",
                            Value::HashMap(_) => "map",
                            Value::Nil => "nil",
                            Value::Boolean(_) => "boolean",
                            Value::Char(_) => "char",
                            Value::Function(_) => "function",
                            Value::FileHandle(_) => "file",
                            Value::DatabaseHandle(_) => "database",    
                            Value::Enum(_, _) => "enum",
                            Value::Struct(_, _) => "struct",
                            Value::GUIFrame(_) => "GUI frame",
                            Value::GUIWidget(_) => "GUI widget",
                        };
                        return Err(format!(
                            "Cannot iterate over type: {}\n\
                             Supported types: array, number (integer), string, map",
                            type_name
                        ));
                    }
                }

                Ok(())
            }
            Statement::Throw(expr) => {
                let error_msg = self.evaluate_expression(expr)?.to_string();

                if self.caught_error.is_some() {
                    Err(error_msg)
                } else {
                    Err(format!("Uncaught error: {}", error_msg))
                }
            }
            Statement::Try(try_block, error_var, catch_block) => {
                let old_error = self.caught_error.take();
                self.caught_error = Some(Value::Nil);

                let result = self.execute_block(try_block);

                if let Err(error_msg) = result {
                    let error_value = Value::String(error_msg);
                    self.caught_error = Some(error_value.clone());
                    self.environment.borrow_mut().define(error_var, error_value);
                    self.execute_block(catch_block)?;
                    self.caught_error = None;
                } else {
                    self.caught_error = old_error;
                }

                Ok(())
            }
            Statement::When(when_case) => {
                let mut matched = false;

                for branch in when_case.cases {
                    let condition_value = self.evaluate_expression(branch.condition)?;

                    if self.is_truthy(&condition_value) {
                        matched = true;
                        self.execute_block(branch.body)?;
                        break;
                    }
                }

                if !matched {
                    if let Some(default_body) = when_case.default_case {
                        self.execute_block(default_body)?;
                    }
                }

                Ok(())
            }
            Statement::For(var_name, start, end, step, body) => {
                let start_val = self.evaluate_expression(start)?;
                let end_val = self.evaluate_expression(end)?;
                let step_val = self.evaluate_expression(step)?;
            
                if let (Value::Number(start_num), Value::Number(end_num), Value::Number(step_num)) =
                    (start_val, end_val, step_val) {
                    let mut i = start_num;
                    
                    let use_var = var_name != "_";
                    
                    if step_num > 0.0 {
                        while i <= end_num {
                            self.should_break = false;
                            self.should_continue = false;
            
                            if use_var {
                                self.environment.borrow_mut().define(var_name.clone(), Value::Number(i));
                            }
                            self.execute_block(body.clone())?;
            
                            if self.should_break {
                                self.should_break = false;
                                break;
                            }
            
                            if self.should_continue {
                                self.should_continue = false;
                                i += step_num;
                                continue;
                            }
            
                            if self.return_value.is_some() {
                                break;
                            }
                            i += step_num;
                        }
                    } else if step_num < 0.0 {
                        while i >= end_num {
                            self.should_break = false;
                            self.should_continue = false;
            
                            if use_var {
                                self.environment.borrow_mut().define(var_name.clone(), Value::Number(i));
                            }
                            self.execute_block(body.clone())?;
            
                            if self.should_break {
                                self.should_break = false;
                                break;
                            }
            
                            if self.should_continue {
                                self.should_continue = false;
                                i += step_num;
                                continue;
                            }
            
                            if self.return_value.is_some() {
                                break;
                            }
                            i += step_num;
                        }
                    }
                } else {
                    return Err("For loop expects numeric values".to_string());
                }
                Ok(())
            }
            Statement::Break => {
                self.should_break = true;
                Ok(())
            }
            Statement::Continue => {
                self.should_continue = true;
                Ok(())
            }
            Statement::Enum(name, variants) => {
                self.environment.borrow_mut().define_enum(name, variants);
                Ok(())
            }
            Statement::PvtLet(name, expr) => {
                let value = self.evaluate_expression(expr)?;
                self.environment.borrow_mut().define_private(name, value);
                Ok(())
            }
            Statement::PvtFunction(name, params, body) => {
                let func = Rc::new(Function {
                    name: name.clone(),
                    params: params.clone(),
                    body: body.clone(),
                    param_count: params.len(),
                    is_private: true,
                    closure_env: None,
                });
                self.environment.borrow_mut().define_private_function(name, func);
                Ok(())
            }
            Statement::Switch(condition, cases, default_case) => {
                let cond_value = self.evaluate_expression(condition)?;
                let mut matched = false;

                for case in cases {
                    let case_value = self.evaluate_expression(case.value)?;

                    if cond_value == case_value {
                        matched = true;
                        self.execute_block(case.body)?;
                        break;
                    }
                }

                if !matched {
                    if let Some(default_body) = default_case {
                        self.execute_block(default_body)?;
                    }
                }

                Ok(())
            }
            Statement::Function(name, params, body, is_private) => {
                let func = Rc::new(Function {
                    name: name.clone(),
                    params: params.clone(),
                    body: body.clone(),
                    param_count: params.len(),
                    is_private: is_private,
                    closure_env: None,
                });
                self.environment.borrow_mut().define_function(name, func);
                Ok(())
            }

            Statement::Return(expr) => {
                let value = if let Some(expr) = expr {
                    self.evaluate_expression(expr)?
                } else {
                    Value::Nil
                };
                self.return_value = Some(value);
                Ok(())
            }
            Statement::Block(statements) => self.execute_block(statements),
        }
    }

    fn execute_block(&mut self, statements: Vec<Statement>) -> Result<(), String> {
        let new_env = Rc::new(RefCell::new(Environment::with_parent(self.environment.clone())));
        let old_env = std::mem::replace(&mut self.environment, new_env);

        for stmt in statements {
            self.execute_statement(stmt)?;

            if self.should_break {
                break;
            }
            if self.should_continue {
                break;
            }
            if self.return_value.is_some() {
                break;
            }
        }

        self.environment = old_env;
        Ok(())
    }

    fn evaluate_expression(&mut self, expr: Expr) -> Result<Value, String> {
        match expr {
            Expr::Literal(lit) => self.evaluate_literal(lit),
            Expr::Variable(name) => {
                self.environment.borrow().get(&name)
                    .ok_or_else(|| format!("Undefined variable: {}", name))
            }
            Expr::Binary(left, op, right) => {
                let left_val = self.evaluate_expression(*left)?;
                let right_val = self.evaluate_expression(*right)?;
                self.evaluate_binary(op, left_val, right_val)
            }
            Expr::Unary(op, expr) => {
                let val = self.evaluate_expression(*expr)?;
                self.evaluate_unary(op, val)
            }
            Expr::Array(elements) => {
                let mut arr = Vec::new();
                for elem in elements {
                    arr.push(self.evaluate_expression(elem)?);
                }
                Ok(Value::Array(arr))
            }
            Expr::GlobalAssignment(name, expr) => {
                let value = self.evaluate_expression(*expr)?;
                self.environment.borrow_mut().set_global(name, value)?;
                Ok(Value::Nil)
            }
            Expr::ParentAssignment(name, expr) => {
                let value = self.evaluate_expression(*expr)?;
                self.environment.borrow_mut().set_parent_scope(name, value)?;
                Ok(Value::Nil)
            }
            Expr::ParentAccess(name) => {
                self.environment.borrow().get_parent_scope(&name)
                    .ok_or_else(|| format!("Parent variable '{}' not found in any scope", name))
            }
            Expr::EnumAccess(enum_name, variant_name) => {
                self.environment.borrow()
                    .get_enum_variant(&enum_name, &variant_name)
                    .ok_or_else(|| format!("Enum '{}::{}' not found", enum_name, variant_name))
            }
            Expr::ModuleVariable(module_name, var_name) => {
                if var_name == "module" {
                    let actual_module = self.module_aliases.get(&module_name)
                        .unwrap_or(&module_name)
                        .clone();

                    if self.aliased_modules.contains(&actual_module) && !self.alias_to_module.contains_key(&module_name) {
                        return Err(format!(
                            "Module '{}' was imported with an alias. Use the alias instead.",
                            actual_module
                        ));
                    }

                    return self.module_to_string(&module_name);
                }

                let actual_module = self.module_aliases.get(&module_name)
                    .unwrap_or(&module_name)
                    .clone();

                if self.aliased_modules.contains(&actual_module) && !self.alias_to_module.contains_key(&module_name) {
                    return Err(format!(
                        "Module '{}' was imported with an alias. Use the alias instead.",
                        actual_module
                    ));
                }

                let module_env = self.loaded_modules.get(&actual_module)
                    .ok_or_else(|| format!(
                        "Module '{}' not loaded. Did you forget to import it?",
                        module_name
                    ))?;

                {
                    let env = module_env.borrow();
                    if env.get_private(&var_name).is_some() {
                        return Err(format!(
                            "Cannot access private variable '{}' from module '{}'",
                            var_name, module_name
                        ));
                    }
                    if env.get(&var_name).is_none() {
                        return Err(format!(
                            "Variable '{}' not found in module '{}'",
                            var_name, module_name
                        ));
                    }
                }

                let value = module_env.borrow().get(&var_name)
                    .ok_or_else(|| format!(
                        "Variable '{}' not found in module '{}'",
                        var_name, module_name
                    ))?;

                Ok(value)
            }
            Expr::ModuleEnumAccess(module_name, enum_name, variant_name) => {
                let actual_module = self.module_aliases.get(&module_name)
                    .unwrap_or(&module_name)
                    .clone();

                if self.aliased_modules.contains(&actual_module) && !self.alias_to_module.contains_key(&module_name) {
                    return Err(format!(
                        "Module '{}' was imported with an alias. Use the alias instead.",
                        actual_module
                    ));
                }

                let module_env = self.loaded_modules.get(&actual_module)
                    .ok_or_else(|| format!(
                        "Module '{}' not loaded. Did you forget to import it?",
                        module_name
                    ))?;

                let env = module_env.borrow();
                if env.get_enum_variant(&enum_name, &variant_name).is_none() {
                    return Err(format!(
                        "Enum '{}::{}' not found in module '{}'",
                        enum_name, variant_name, module_name
                    ));
                }

                let value = env.get_enum_variant(&enum_name, &variant_name)
                    .ok_or_else(|| format!(
                        "Enum '{}::{}' not found in module '{}'",
                        enum_name, variant_name, module_name
                    ))?;

                Ok(value)
            }
            Expr::ModuleCall(module_name, func_name, args) => {
                let actual_module = self.module_aliases.get(&module_name)
                    .unwrap_or(&module_name)
                    .clone();

                if self.aliased_modules.contains(&actual_module) && !self.alias_to_module.contains_key(&module_name) {
                    return Err(format!(
                        "Module '{}' was imported with an alias. Use the alias instead.",
                        actual_module
                    ));
                }

                let module_env = self.loaded_modules.get(&actual_module)
                    .ok_or_else(|| format!("Module '{}' not loaded. Did you forget to import it?", module_name))?;

                {
                    let env = module_env.borrow();
                    if env.get_private_function(&func_name, args.len()).is_some() {
                        return Err(format!(
                            "Cannot access private function '{}' from module '{}'",
                            func_name, module_name
                        ));
                    }
                    if env.get_function(&func_name, args.len()).is_none() {
                        return Err(format!(
                            "Function '{}' with {} arguments not found in module '{}'",
                            func_name, args.len(), module_name
                        ));
                    }
                }

                let old_env = std::mem::replace(&mut self.environment, module_env.clone());

                let call_expr = Expr::Call(func_name, args);
                let result = self.evaluate_expression(call_expr)?;

                self.environment = old_env;

                Ok(result)
            }
            Expr::Lambda(params, body) => {
                let param_count = params.len();
                let func = Rc::new(Function {
                    name: "lambda".to_string(),
                    params: params,
                    body: body,
                    param_count: param_count,
                    is_private: false,
                    closure_env: Some(self.environment.clone()),
                });
                Ok(Value::Function(func))
            }
            Expr::Ternary(condition, true_expr, false_expr) => {
                let cond_val = self.evaluate_expression(*condition)?;
                if self.is_truthy(&cond_val) {
                    self.evaluate_expression(*true_expr)
                } else {
                    self.evaluate_expression(*false_expr)
                }
            }
            Expr::Struct(name, fields) => {
                let mut field_values = HashMap::new();
                for (field_name, field_expr) in fields {
                    let value = self.evaluate_expression(field_expr)?;
                    field_values.insert(field_name, value);
                }
                self.environment.borrow_mut().define(name.clone(), Value::Struct(name, field_values));
                Ok(Value::Nil)
            }
            Expr::StructInstance(struct_name, _instance_name) => {
                let struct_def = self.environment.borrow().get(&struct_name)
                    .ok_or_else(|| format!("Struct '{}' not found", struct_name))?;

                match struct_def {
                    Value::Struct(_, fields) => {
                        let mut instance_fields = HashMap::new();
                        for (key, value) in fields {
                            instance_fields.insert(key.clone(), value.clone());
                        }
                        let instance_id = format!("{}_{}", struct_name, self.struct_counter);
                        self.struct_counter += 1;
                        Ok(Value::Struct(instance_id, instance_fields))
                    }
                    _ => Err(format!("'{}' is not a struct", struct_name)),
                }
            }
            Expr::StructAccess(obj_expr, field_name) => {
                let obj = self.evaluate_expression(*obj_expr)?;
                match obj {
                    Value::Struct(_, fields) => {
                        fields.get(&field_name)
                            .cloned()
                            .ok_or_else(|| format!("Field '{}' not found in struct", field_name))
                    }
                    _ => Err("Cannot access field on non-struct value".to_string()),
                }
            }
            Expr::StructAssignment(obj_expr, field_name, value_expr) => {
                let value = self.evaluate_expression(*value_expr)?;
                let obj = self.evaluate_expression(*obj_expr.clone())?;

                match obj {
                    Value::Struct(instance_id, mut fields) => {
                        fields.insert(field_name, value);

                        let updated_struct = Value::Struct(instance_id, fields);

                        let var_name = match *obj_expr {
                            Expr::Variable(name) => Some(name),
                            Expr::StructAccess(parent_obj, _) => {
                                let mut current = &*parent_obj;
                                while let Expr::StructAccess(parent, _) = current {
                                    current = &*parent;
                                }
                                if let Expr::Variable(name) = current {
                                    Some(name.clone())
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        };

                        if let Some(name) = var_name {
                            self.environment.borrow_mut().set(name, updated_struct.clone())?;
                        }

                        Ok(updated_struct)
                    }
                    _ => Err("Cannot assign to field on non-struct value".to_string()),
                }
            }
            Expr::HashMap(pairs) => {
                let mut map = HashMap::new();
                for (key_expr, value_expr) in pairs {
                    let key_value = self.evaluate_expression(key_expr)?;
                    let value = self.evaluate_expression(value_expr)?;
                    map.insert(key_value, value);
                }
                Ok(Value::HashMap(map))
            }
            Expr::ArrayIndex(arr_expr, idx_expr) => {
                let arr_expr_clone = arr_expr.clone();
                let arr_val = self.evaluate_expression(*arr_expr)?;
                let idx_val = self.evaluate_expression(*idx_expr)?;

                match (arr_val, idx_val) {
                    (Value::Array(mut arr), Value::Number(idx)) => {
                        let idx_usize = idx as usize;
                        if idx_usize >= arr.len() {
                            arr.resize(idx_usize + 1, Value::Nil);
                            if let Expr::Variable(name) = *arr_expr_clone {
                                self.environment.borrow_mut().set(name, Value::Array(arr.clone()))?;
                            }
                        }
                        Ok(arr[idx_usize].clone())
                    }
                    _ => Err("Cannot index non-array with non-number".to_string()),
                }
            }
            Expr::Call(name, args) => {
                if let Some(builtin) = self.get_builtin_function(&name) {
                    return builtin(self, args);
                }

                let lambda_opt = self.environment.borrow().get(&name);
                if let Some(Value::Function(func_rc)) = lambda_opt {
                    if args.len() != func_rc.params.len() {
                        return Err(format!("Function expects {} arguments, got {}",
                                          func_rc.params.len(), args.len()));
                    }

                    let mut evaluated_args = Vec::new();
                    for arg in args {
                        evaluated_args.push(self.evaluate_expression(arg)?);
                    }

                    let closure_env = func_rc.closure_env.clone();

                    let new_env = if let Some(env) = closure_env {
                        Rc::new(RefCell::new(Environment::with_parent(env)))
                    } else {
                        Rc::new(RefCell::new(Environment::with_parent(self.environment.clone())))
                    };

                    let old_env = std::mem::replace(&mut self.environment, new_env);

                    for (param, arg_val) in func_rc.params.iter().zip(evaluated_args.iter()) {
                        self.environment.borrow_mut().define(param.clone(), arg_val.clone());
                    }

                    let old_return = self.return_value.take();
                    let mut result = Value::Nil;

                    for stmt in func_rc.body.iter() {
                        self.execute_statement(stmt.clone())?;
                        if let Some(ret) = self.return_value.take() {
                            result = ret;
                            break;
                        }
                    }

                    self.return_value = old_return;
                    self.environment = old_env;
                    return Ok(result);
                }

                let arg_count = args.len();
                let func_rc = self.get_function(&name, arg_count)
                    .ok_or_else(|| format!("Function '{}' with {} arguments not found", name, arg_count))?;

                if args.len() != func_rc.params.len() {
                    return Err(format!("Function {} expects {} arguments, got {}",
                                      name, func_rc.params.len(), args.len()));
                }

                let new_env = Rc::new(RefCell::new(Environment::with_parent(self.environment.clone())));
                let old_env = std::mem::replace(&mut self.environment, new_env);

                for (param, arg) in func_rc.params.iter().zip(args.iter()) {
                    let arg_val = self.evaluate_expression(arg.clone())?;
                    self.environment.borrow_mut().define(param.clone(), arg_val);
                }

                let old_return = self.return_value.take();
                let mut result = Value::Nil;

                for stmt in func_rc.body.iter() {
                    self.execute_statement(stmt.clone())?;
                    if let Some(ret) = self.return_value.take() {
                        result = ret;
                        break;
                    }
                }

                self.return_value = old_return;
                self.environment = old_env;
                Ok(result)
            }
            Expr::Assignment(name, expr, reassign) => {
                let value = self.evaluate_expression(*expr)?;
                if reassign {
                    self.environment.borrow_mut().assign(name, value)?;
                } else {
                    self.environment.borrow_mut().define(name, value);
                }
                Ok(Value::Nil)
            }
            Expr::ArrayAssignment(arr_expr, idx_expr, val_expr) => {
                let arr_expr_clone = arr_expr.clone();
                let arr_val = self.evaluate_expression(*arr_expr)?;
                let idx_val = self.evaluate_expression(*idx_expr)?;
                let val = self.evaluate_expression(*val_expr)?;

                match (arr_val, idx_val) {
                    (Value::Array(mut arr), Value::Number(idx)) => {
                        let idx_usize = idx as usize;
                        if idx_usize >= arr.len() {
                            arr.resize(idx_usize + 1, Value::Nil);
                        }
                        arr[idx_usize] = val;

                        if let Expr::Variable(name) = *arr_expr_clone {
                            self.environment.borrow_mut().set(name, Value::Array(arr))?;
                        }
                        Ok(Value::Nil)
                    }
                    _ => Err("Cannot assign to non-array index".to_string()),
                }
            }
        }
    }

    fn evaluate_literal(&self, lit: Literal) -> Result<Value, String> {
        match lit {
            Literal::Number(n) => Ok(Value::Number(n)),
            Literal::String(s) => Ok(Value::String(s)),
            Literal::Char(c) => Ok(Value::Char(c)),
            Literal::Boolean(b) => Ok(Value::Boolean(b)),
            Literal::Nil => Ok(Value::Nil),
        }
    }

    fn evaluate_binary(&self, op: BinaryOp, left: Value, right: Value) -> Result<Value, String> {
        match op {
            BinaryOp::Add => {
                match (left, right) {
                    (Value::Number(l), Value::Number(r)) => Ok(Value::Number(l + r)),
                    (Value::String(l), Value::String(r)) => Ok(Value::String(l + &r)),
                    (Value::String(l), r) => Ok(Value::String(l + &r.to_string())),
                    (l, Value::String(r)) => Ok(Value::String(l.to_string() + &r)),
                    (Value::FileHandle(_), _) => Err("Cannot add file handles".to_string()),
                    (_, Value::FileHandle(_)) => Err("Cannot add file handles".to_string()),
                    _ => Err("Cannot add non-numbers or non-strings".to_string()),
                }
            }
            BinaryOp::Sub => {
                match (left, right) {
                    (Value::Number(l), Value::Number(r)) => Ok(Value::Number(l - r)),
                    (Value::FileHandle(_), _) => Err("Cannot subtract file handles".to_string()),
                    (_, Value::FileHandle(_)) => Err("Cannot subtract file handles".to_string()),
                    _ => Err("Subtraction requires numbers".to_string()),
                }
            }
            BinaryOp::Mul => {
                match (left, right) {
                    (Value::Number(l), Value::Number(r)) => Ok(Value::Number(l * r)),

                    (Value::String(s), Value::Number(n)) => {
                        let times = n as usize;
                        if times == 0 {
                            Ok(Value::String(String::new()))
                        } else {
                            Ok(Value::String(s.repeat(times)))
                        }
                    }

                    (Value::Number(n), Value::String(s)) => {
                        let times = n as usize;
                        if times == 0 {
                            Ok(Value::String(String::new()))
                        } else {
                            Ok(Value::String(s.repeat(times)))
                        }
                    }

                    (Value::FileHandle(_), _) => Err("Cannot multiply file handles".to_string()),
                    (_, Value::FileHandle(_)) => Err("Cannot multiply file handles".to_string()),
                    _ => Err("Multiplication requires numbers or string * number".to_string()),
                }
            }
            BinaryOp::Div => {
                match (left, right) {
                    (Value::Number(l), Value::Number(r)) => {
                        if r == 0.0 {
                            Err("Division by zero".to_string())
                        } else {
                            Ok(Value::Number(l / r))
                        }
                    }
                    (Value::FileHandle(_), _) => Err("Cannot divide file handles".to_string()),
                    (_, Value::FileHandle(_)) => Err("Cannot divide file handles".to_string()),
                    _ => Err("Division requires numbers".to_string()),
                }
            }
            BinaryOp::Mod => {
                match (left, right) {
                    (Value::Number(l), Value::Number(r)) => {
                        if r == 0.0 {
                            Err("Modulo by zero".to_string())
                        } else {
                            Ok(Value::Number(l % r))
                        }
                    }
                    (Value::FileHandle(_), _) => Err("Cannot modulo file handles".to_string()),
                    (_, Value::FileHandle(_)) => Err("Cannot modulo file handles".to_string()),
                    _ => Err("Modulo requires numbers".to_string()),
                }
            }
            BinaryOp::Eq => {
                match (&left, &right) {
                    (Value::FileHandle(l), Value::FileHandle(r)) => Ok(Value::Boolean(l == r)),
                    (Value::Struct(l_name, l_fields), Value::Struct(r_name, r_fields)) => {
                        Ok(Value::Boolean(l_name == r_name && l_fields == r_fields))
                    }
                    _ => Ok(Value::Boolean(left == right)),
                }
            }
            BinaryOp::Neq => {
                match (&left, &right) {
                    (Value::FileHandle(l), Value::FileHandle(r)) => Ok(Value::Boolean(l != r)),
                    (Value::Enum(l_name, l_variant), Value::Enum(r_name, r_variant)) => {
                        Ok(Value::Boolean(l_name != r_name || l_variant != r_variant))
                    }
                    _ => Ok(Value::Boolean(left != right)),
                }
            }
            BinaryOp::Lt => {
                match (left, right) {
                    (Value::Number(l), Value::Number(r)) => Ok(Value::Boolean(l < r)),
                    (Value::FileHandle(_), _) => Err("Cannot compare file handles with <".to_string()),
                    (_, Value::FileHandle(_)) => Err("Cannot compare file handles with <".to_string()),
                    _ => Err("Comparison requires numbers".to_string()),
                }
            }
            BinaryOp::Gt => {
                match (left, right) {
                    (Value::Number(l), Value::Number(r)) => Ok(Value::Boolean(l > r)),
                    (Value::FileHandle(_), _) => Err("Cannot compare file handles with >".to_string()),
                    (_, Value::FileHandle(_)) => Err("Cannot compare file handles with >".to_string()),
                    _ => Err("Comparison requires numbers".to_string()),
                }
            }
            BinaryOp::Le => {
                match (left, right) {
                    (Value::Number(l), Value::Number(r)) => Ok(Value::Boolean(l <= r)),
                    (Value::FileHandle(_), _) => Err("Cannot compare file handles with <=".to_string()),
                    (_, Value::FileHandle(_)) => Err("Cannot compare file handles with <=".to_string()),
                    _ => Err("Comparison requires numbers".to_string()),
                }
            }
            BinaryOp::Ge => {
                match (left, right) {
                    (Value::Number(l), Value::Number(r)) => Ok(Value::Boolean(l >= r)),
                    (Value::FileHandle(_), _) => Err("Cannot compare file handles with >=".to_string()),
                    (_, Value::FileHandle(_)) => Err("Cannot compare file handles with >=".to_string()),
                    _ => Err("Comparison requires numbers".to_string()),
                }
            }
            BinaryOp::And => {
                let left_truthy = self.is_truthy(&left);
                let right_truthy = self.is_truthy(&right);
                Ok(Value::Boolean(left_truthy && right_truthy))
            }
            BinaryOp::Or => {
                let left_truthy = self.is_truthy(&left);
                let right_truthy = self.is_truthy(&right);
                Ok(Value::Boolean(left_truthy || right_truthy))
            }
            BinaryOp::BitAnd => {
                match (left, right) {
                    (Value::Number(l), Value::Number(r)) => Ok(Value::Number((l as i64 & r as i64) as f64)),
                    (Value::FileHandle(_), _) => Err("Cannot bitwise AND file handles".to_string()),
                    (_, Value::FileHandle(_)) => Err("Cannot bitwise AND file handles".to_string()),
                    _ => Err("Bitwise operations require integers".to_string()),
                }
            }
            BinaryOp::BitOr => {
                match (left, right) {
                    (Value::Number(l), Value::Number(r)) => Ok(Value::Number((l as i64 | r as i64) as f64)),
                    (Value::FileHandle(_), _) => Err("Cannot bitwise OR file handles".to_string()),
                    (_, Value::FileHandle(_)) => Err("Cannot bitwise OR file handles".to_string()),
                    _ => Err("Bitwise operations require integers".to_string()),
                }
            }
            BinaryOp::BitXor => {
                match (left, right) {
                    (Value::Number(l), Value::Number(r)) => Ok(Value::Number((l as i64 ^ r as i64) as f64)),
                    (Value::FileHandle(_), _) => Err("Cannot bitwise XOR file handles".to_string()),
                    (_, Value::FileHandle(_)) => Err("Cannot bitwise XOR file handles".to_string()),
                    _ => Err("Bitwise operations require integers".to_string()),
                }
            }
        }
    }

    fn evaluate_unary(&self, op: UnaryOp, val: Value) -> Result<Value, String> {
        match op {
            UnaryOp::Neg => {
                if let Value::Number(n) = val {
                    Ok(Value::Number(-n))
                } else {
                    Err("Negation requires a number".to_string())
                }
            }
            UnaryOp::Not => Ok(Value::Boolean(!self.is_truthy(&val))),
            UnaryOp::BitNot => {
                if let Value::Number(n) = val {
                    Ok(Value::Number((!(n as i64)) as f64))
                } else {
                    Err("Bitwise NOT requires an integer".to_string())
                }
            }
        }
    }

    fn is_truthy(&self, val: &Value) -> bool {
        match val {
            Value::Boolean(b) => *b,
            Value::Nil => false,
            Value::Number(n) => *n != 0.0,
            _ => true,
        }
    }
}

extern "C" {
    fn get_time() -> f64;
    fn get_random(min: i32, max: i32) -> i32;
    fn sleep_seconds(seconds: i32) -> i32;
    fn date(format: *const i8) -> *const i8;
    fn sin_f(x: f64) -> f64;
    fn cos_f(x: f64) -> f64;
    fn tan_f(x: f64) -> f64;
    fn asin_f(x: f64) -> f64;
    fn acos_f(x: f64) -> f64;
    fn atan_f(x: f64) -> f64;
    fn atan2_f(y: f64, x: f64) -> f64;

    fn csc_f(x: f64) -> f64;
    fn sec_f(x: f64) -> f64;
    fn cot_f(x: f64) -> f64;

    fn sinh_f(x: f64) -> f64;
    fn cosh_f(x: f64) -> f64;
    fn tanh_f(x: f64) -> f64;
    fn asinh_f(x: f64) -> f64;
    fn acosh_f(x: f64) -> f64;
    fn atanh_f(x: f64) -> f64;

    fn exp_f(x: f64) -> f64;
    fn log_f(x: f64) -> f64;
    fn log10_f(x: f64) -> f64;
    fn log2_f(x: f64) -> f64;

    fn pow_f(base: f64, exp: f64) -> f64;
    fn sqrt_f(x: f64) -> f64;
    fn cbrt_f(x: f64) -> f64;
    fn hypot_f(x: f64, y: f64) -> f64;

    fn abs_f(x: f64) -> f64;

    fn ceil_f(x: f64) -> f64;
    fn floor_f(x: f64) -> f64;
    fn round_f(x: f64) -> f64;
    fn trunc_f(x: f64) -> f64;

    fn erf_f(x: f64) -> f64;
    fn erfc_f(x: f64) -> f64;

    fn gamma_f(x: f64) -> f64;

    fn pi_f() -> f64;
    fn e_f() -> f64;

    fn factorial_f(n: i32) -> f64;
    fn permutation_f(n: i32, r: i32) -> f64;
    fn combination_f(n: i32, r: i32) -> f64;
    fn gcd_ll(a: i64, b: i64) -> i64;
    fn lcm_ll(a: i64, b: i64) -> i64;

    // SQLite functions
    fn db_open(filename: *const i8) -> *mut std::ffi::c_void;
    fn db_close(db: *mut std::ffi::c_void);
    fn db_execute(db: *mut std::ffi::c_void, sql: *const i8, err_buf: *mut i8, err_buf_size: i32) -> i32;
    fn db_query(db: *mut std::ffi::c_void, sql: *const i8, params: *mut *const i8, param_count: i32) -> *mut i8;
    fn db_execute_params(db: *mut std::ffi::c_void, sql: *const i8, params: *mut *const i8, param_count: i32) -> i32;
    fn db_last_insert_id(db: *mut std::ffi::c_void) -> i64;
    fn db_error(db: *mut std::ffi::c_void) -> *const i8;
    fn db_free_result(data: *mut i8);

    fn system_f(argument: *const i8) -> i32;

    fn regex_test(pattern: *const i8, text: *const i8, flags: i32) -> i32;
    fn regex_find(pattern: *const i8, text: *const i8, group: i32, flags: i32) -> *const i8;
    fn regex_replace(pattern: *const i8, text: *const i8, replacement: *const i8, flags: i32) -> *const i8;
    fn regex_split(pattern: *const i8, text: *const i8, flags: i32, count: *mut i32) -> *mut *const i8;
    fn free_string(ptr: *const i8);
    fn free_split_results(results: *mut *const i8, count: i32);
    fn exit_program(flag: i32) -> !;
    fn http_get(url: *const i8) -> *const i8;
    fn http_post(url: *const i8, post_data: *const i8) -> *const i8;
    fn http_request(method: *const i8, url: *const i8, headers: *const i8, body: *const i8) -> *const i8;
    fn json_encode(input: *const i8) -> *const i8;
    fn json_decode(input: *const i8) -> *const i8;
    fn range(start: i32, end: i32, step: i32) -> *const i8;
    fn uuid_v4() -> *const i8;

    fn strcmp_prism(a: *const i8, b: *const i8) -> i32;
}

#[cfg(gui)]
extern "C" {
    fn gui_frame_new(title: *const i8, width: i32, height: i32, x: i32, y: i32) -> *mut std::ffi::c_void;
    fn gui_label_new(frame: *mut std::ffi::c_void, text: *const i8, x: i32, y: i32, font_size: i32) -> *mut std::ffi::c_void;
    fn gui_button_new(frame: *mut std::ffi::c_void, text: *const i8, x: i32, y: i32, width: i32, height: i32) -> *mut std::ffi::c_void;
    fn gui_button_set_callback(button: *mut std::ffi::c_void, callback_str: *const i8);
    fn gui_label_set_text(label: *mut std::ffi::c_void, text: *const i8);
    fn gui_label_get_text(label: *mut std::ffi::c_void) -> *const i8;
    fn gui_auto_widget_scale(frame: *mut std::ffi::c_void, enabled: i32);
    fn gui_start(frame: *mut std::ffi::c_void);
    fn gui_quit();
    fn gui_set_callback(callback: extern "C" fn(*const i8));
}
