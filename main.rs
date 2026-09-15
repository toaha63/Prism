mod sha256sum;
mod sha512sum;
mod lexer;
mod parser;
mod interpreter;
mod pacman;

use std::env;
use std::fs;
use std::process;
use std::net::{TcpListener, TcpStream};
use std::io::{Read, Write};
use std::thread;
use std::collections::HashMap;

use lexer::Lexer;
use parser::Parser;
use interpreter::Interpreter;
use pacman::{handle_install, handle_uninstall, handle_list, handle_search, handle_update, handle_update_database};


fn main()
{
    let args: Vec<String> = env::args().collect();

    if args.len() < 2
    {
        eprintln!("Prism Language Interpreter v1.0.5");
        eprintln!("");
        eprintln!("USAGE:");
        eprintln!("    prism <command> [arguments]");
        eprintln!("");
        eprintln!("COMMANDS:");
        eprintln!("    <file.prism>              Execute a Prism script");
        eprintln!("    install <pkg>[@version]   Install a package from the registry");
        eprintln!("    uninstall <pkg>           Remove an installed package");
        eprintln!("    list                      Show all installed packages");
        eprintln!("    search <query>            Search the package registry");
        eprintln!("    update [pkg]              Update packages to latest versions");
        eprintln!("    update --database         Refresh the package database");
        eprintln!("    -S <host:port>            Start the web development server");
        eprintln!("");
        eprintln!("OPTIONS:");
        eprintln!("    -v, --version             Show version information");
        eprintln!("    -h, --help                Show this help message");
        eprintln!("");
        eprintln!("EXAMPLES:");
        eprintln!("    prism app.prism");
        eprintln!("    prism install uuid@1.0.0");
        eprintln!("    prism -S 127.0.0.1:8080");
        process::exit(1);
    }

    match args[1].as_str()
    {
        "-v" | "--version" =>
        {
            println!("Prism v1.0.5");
            #[cfg(gui)]
            println!("Build: with GUI (GTK)");
            #[cfg(not(gui))]
            println!("Build: no GUI");
            process::exit(0);
        }
        "-h" | "--help" =>
        {
            eprintln!("Prism Language Interpreter v1.0.5");
            eprintln!("");
            eprintln!("USAGE:");
            eprintln!("    prism <command> [arguments]");
            eprintln!("");
            eprintln!("COMMANDS:");
            eprintln!("    <file.prism>              Execute a Prism script");
            eprintln!("    install <pkg>[@version]   Install a package from the registry");
            eprintln!("    uninstall <pkg>           Remove an installed package");
            eprintln!("    list                      Show all installed packages");
            eprintln!("    search <query>            Search the package registry");
            eprintln!("    update [pkg]              Update packages to latest versions");
            eprintln!("    update --database         Refresh the package database");
            eprintln!("    -S <host:port>            Start the web development server");
            eprintln!("");
            eprintln!("OPTIONS:");
            eprintln!("    -v, --version             Show version information");
            eprintln!("    -h, --help                Show this help message");
            eprintln!("");
            eprintln!("EXAMPLES:");
            eprintln!("    prism app.prism");
            eprintln!("    prism install uuid@1.0.0");
            eprintln!("    prism -S 127.0.0.1:8080");
            process::exit(0);
        }
        "install" =>
        {
            if args.len() < 3
            {
                eprintln!("Error: Package name required");
                eprintln!("Usage: prism install <package>[@version]");
                process::exit(1);
            }
            handle_install(&args[2]);
        }
        "uninstall" =>
        {
            if args.len() < 3
            {
                eprintln!("Error: Package name required");
                eprintln!("Usage: prism uninstall <package>");
                process::exit(1);
            }
            handle_uninstall(&args[2]);
        }
        "list" =>
        {
            handle_list();
        }
        "search" =>
         {
            if args.len() < 3
            {
                eprintln!("Error: Search query required");
                eprintln!("Usage: prism search <query>");
                process::exit(1);
            }
            handle_search(&args[2]);
        }
        "update" =>
         {
            let mut force_refresh = false;
            let mut package = None;
            
            let mut i = 2;
            while i < args.len()
            {
                match args[i].as_str()
                {
                    "--database" | "-d" =>
                     {
                        // Only update database.json
                        handle_update_database();
                        return;
                    }
                    "--refresh" | "-r" =>
                    {
                        force_refresh = true;
                        i += 1;
                    }
                    _ =>
                    {
                        package = Some(args[i].as_str());
                        i += 1;
                    }
                }
            }
            
            handle_update(package, force_refresh);
        }
        "-S" =>
        {
            if args.len() < 3
            {
                eprintln!("Error: Address required");
                eprintln!("Usage: prism -S <host:port>");
                process::exit(1);
            }
            let addr = &args[2];
            start_server(addr);
        }
        _ =>
        {
            let filename = &args[1];
            let terminal_args: Vec<String> = args[2..].to_vec();
            run_file_with_args(filename, terminal_args);
        }
    }
}

fn run_file_with_args(filename: &str, terminal_args: Vec<String>)
{
    let source = match fs::read_to_string(filename)
    {
        Ok(content) => content,
        Err(e) =>
        {
            eprintln!("Error reading file {}: {}", filename, e);
            process::exit(1);
        }
    };

    let lexer = Lexer::new(&source);
    let mut parser = Parser::new(lexer);

    let ast = match parser.parse_program()
    {
        Ok(ast) => ast,
        Err(e) =>
        {
            eprintln!("Parse error: {}", e);
            process::exit(1);
        }
    };

    let mut interpreter = Interpreter::with_terminal_args(terminal_args);

    if let Err(e) = interpreter.interpret(ast)
    {
        eprintln!("Runtime error: {}", e);
        process::exit(1);
    }
}

fn start_server(addr: &str)
{
    let listener = TcpListener::bind(addr).expect("Failed to bind to address");
    println!("{} Prism Development Server running at http://{}", now_str(), addr);
    println!("Press Ctrl+C to stop");

    for stream in listener.incoming()
    {
        match stream
        {
            Ok(stream) =>
            {
                thread::spawn(|| {
                    handle_connection(stream);
                });
            }
            Err(e) =>
            {
                eprintln!("Connection failed: {}", e);
            }
        }
    }
}

fn handle_connection(mut stream: TcpStream)
{
    let mut buffer = [0; 8192];
    let bytes_read = stream.read(&mut buffer).unwrap_or(0);

    if bytes_read == 0
    {
        return;
    }

    let request = String::from_utf8_lossy(&buffer[..bytes_read]);

    let (method, path, headers, body) = parse_http_request(&request);

    // Strip query string (everything after '?')
    let clean_path = if let Some(q_pos) = path.find('?')
    {
        &path[..q_pos]
    }
    else
    {
        &path[..]
    };

    let file_path = if clean_path == "/"
    {
        "index.prism".to_string()
    }
    else
    {
        clean_path.trim_start_matches('/').to_string()
    };

    if !std::path::Path::new(&file_path).exists()
    {
        send_response(&mut stream, 404, "Not Found", "404 - File not found");
        return;
    }

    let form_data = if method == "GET"
    {
        parse_query_string(&path)
    }
    else if method == "POST"
    {
        let content_type = headers.get("content-type").map(|s| s.as_str()).unwrap_or("");
        if content_type.contains("application/x-www-form-urlencoded")
        {
            parse_form_data(&body)
        }
        else
        {
            HashMap::new()
        }
    }
    else
    {
        HashMap::new()
    };

    match execute_prism_file_with_data(&file_path, &form_data, &method)
    {
        Ok(html) =>
        {
            send_response(&mut stream, 200, "OK", &html);
        }
        Err(e) =>
        {
            eprintln!("Error executing {}: {}", file_path, e);
            send_response(&mut stream, 500, "Internal Server Error", &format!("500 - Error: {}", e));
        }
    }
}

fn parse_http_request(request: &str) -> (String, String, HashMap<String, String>, String)
{
    let mut lines = request.lines();
    let first_line = lines.next().unwrap_or("");
    let parts: Vec<&str> = first_line.split_whitespace().collect();

    let method = if parts.len() >= 1 { parts[0].to_string() } else { "GET".to_string() };
    let path = if parts.len() >= 2 { parts[1].to_string() } else { "/".to_string() };

    let mut headers = HashMap::new();
    let mut body_start = false;
    let mut body = String::new();

    for line in lines
    {
        if body_start
        {
            body.push_str(line);
            body.push_str("\n");
        }
        else if line.is_empty()
        {
            body_start = true;
        }
        else if let Some(colon_pos) = line.find(':')
        {
            let key = line[..colon_pos].trim().to_lowercase();
            let value = line[colon_pos + 1..].trim().to_string();
            headers.insert(key, value);
        }
    }

    (method, path, headers, body.trim().to_string())
}

fn parse_query_string(path: &str) -> HashMap<String, String>
{
    let mut params = HashMap::new();

    if let Some(query_start) = path.find('?')
    {
        let query = &path[query_start + 1..];
        for pair in query.split('&')
        {
            if let Some(eq_pos) = pair.find('=')
            {
                let key = url_decode(&pair[..eq_pos]);
                let value = url_decode(&pair[eq_pos + 1..]);
                params.insert(key, value);
            }
        }
    }

    params
}

fn parse_form_data(body: &str) -> HashMap<String, String>
{
    let mut params = HashMap::new();

    for pair in body.split('&')
    {
        if let Some(eq_pos) = pair.find('=')
         {
            let key = url_decode(&pair[..eq_pos]);
            let value = url_decode(&pair[eq_pos + 1..]);
            params.insert(key, value);
        }
    }

    params
}

fn url_decode(s: &str) -> String
{
    let mut result = String::new();
    let mut chars = s.chars();

    while let Some(c) = chars.next()
    {
        if c == '%'
        {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16)
            {
                result.push(byte as char);
            }
        }
        else if c == '+'
        {
            result.push(' ');
        }
        else
        {
            result.push(c);
        }
    }

    result
}

fn send_response(stream: &mut TcpStream, status_code: u16, status_text: &str, body: &str)
{
    let response = format!(
        "HTTP/1.1 {} {}\r\n\
         Content-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n\
         {}",
        status_code, status_text, body.len(), body
    );

    stream.write_all(response.as_bytes()).unwrap();
    stream.flush().unwrap();
}

fn process_html_template(template: &str, form_data: &HashMap<String, String>, method: &str) -> Result<String, String>
{
    let mut output = String::new();
    let mut remaining = template;

    let mut interpreter = Interpreter::with_server_data(form_data, method);

    while !remaining.is_empty()
    {
        if let Some(tag_start) = remaining.find("<?prism")
        {
            output.push_str(&remaining[..tag_start]);
            remaining = &remaining[tag_start + 7..];

            while !remaining.is_empty() && remaining.chars().next().unwrap().is_whitespace()
            {
                remaining = &remaining[1..];
            }

            if let Some(tag_end) = remaining.find("?>")
            {
                let code = &remaining[..tag_end];
                let code_trimmed = code.trim();

                if !code_trimmed.is_empty()
                {
                    match execute_prism_code_in_interpreter(&mut interpreter, code_trimmed)
                    {
                        Ok(result) => output.push_str(&result),
                        Err(e) => return Err(format!("Error in tag: {}", e)),
                    }
                }

                remaining = &remaining[tag_end + 2..];
            }
            else
            {
                return Err("Unclosed <?prism tag".to_string());
            }
        }
        else
        {
            output.push_str(remaining);
            break;
        }
    }

    Ok(output)
}

fn execute_prism_code_in_interpreter(interpreter: &mut Interpreter, code: &str) -> Result<String, String>
{
    interpreter.clear_server_output();
    let lexer = Lexer::new(code);
    let mut parser = Parser::new(lexer);
    let ast = parser.parse_program()?;

    interpreter.interpret(ast)?;

    Ok(interpreter.get_server_output())
}

fn execute_prism_file_with_data(filename: &str, form_data: &HashMap<String, String>, method: &str) -> Result<String, String>
{
    let source = fs::read_to_string(filename)
        .map_err(|e| format!("Failed to read file: {}", e))?;
    
    if source.contains("<?prism") && source.contains("?>")
    {
        process_html_template(&source, form_data, method)
    }
    else
    {
        let mut interpreter = Interpreter::with_server_data(form_data, method);
        let lexer = Lexer::new(&source);
        let mut parser = Parser::new(lexer);
        let ast = parser.parse_program()?;
        interpreter.interpret(ast)?;
        Ok(interpreter.get_server_output())
    }
}

fn now_str() -> String
{
    let fmt = std::ffi::CString::new("[D M d H:i:s Y]").unwrap();
    unsafe
   {
        let ptr = date(fmt.as_ptr() as *const i8);
        if ptr.is_null()
        {
            return "[unknown time]".to_string();
        }
        let s = std::ffi::CStr::from_ptr(ptr as *const std::os::raw::c_char)
            .to_string_lossy()
            .into_owned();
        free_string(ptr as *const i8);
        s
    }
}

extern "C"
{
    fn date(format: *const i8) -> *const i8;
    fn free_string(ptr: *const i8);
}