// pacman.rs - Prism Package Manager

use std::fs;
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::io::Write;

// ========== EXTERNAL C FUNCTIONS ==========
extern "C" {
    fn download_to_file(url: *const std::os::raw::c_char, output_path: *const std::os::raw::c_char) -> i32;
    fn unzip_file(zip_path: *const std::os::raw::c_char, dest_dir: *const std::os::raw::c_char) -> i32;
}

// ========== DATA STRUCTURES ==========
#[derive(Debug, Clone)]
pub struct PackageIndex {
    pub name: String,
    pub version: String,
    pub available_versions: Vec<String>,
    pub description: String,
    pub main: String,
    pub author: Option<String>,
    pub license: Option<String>,
    pub dependencies: HashMap<String, String>,
    pub repo_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LockedPackage {
    pub version: String,
    pub resolved_url: String,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct LockFile {
    pub packages: HashMap<String, LockedPackage>,
}

pub struct Pacman {
    libs_dir: PathBuf,
    cache_dir: PathBuf,
    lock_file: PathBuf,
    database_file: PathBuf,
    database_url: String,
}

// ========== IMPLEMENTATION ==========
impl Pacman {
    pub fn new() -> Result<Self, String> {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".to_string());

        let root = Path::new(&home).join("prism-libs");
        let libs_dir = root.clone();
        let cache_dir = root.join(".cache");
        let lock_file = root.join("deps.lock");
        let database_file = root.join("database.json");

        fs::create_dir_all(&libs_dir)
            .map_err(|e| format!("Failed to create libs dir: {}", e))?;
        fs::create_dir_all(&cache_dir)
            .map_err(|e| format!("Failed to create cache dir: {}", e))?;

        Ok(Pacman {
            libs_dir,
            cache_dir,
            lock_file,
            database_file,
            database_url: "https://raw.githackusercontent.com/toaha63/prism-library-database/main/database.json".to_string(),
        })
    }

    pub fn install(&self, spec: &str) -> Result<(), String> {
        let (name, version) = self.parse_spec(spec);
        println!("Installing {}@{} ...", name, version);

        let db = self.fetch_database(false)?;
        let pkg = self.find_package(&db, &name, &version)?;

        let install_dir = self.libs_dir.join(&name);
        if install_dir.exists() {
            if let Ok(existing) = self.load_package_index(&install_dir) {
                if existing.version == pkg.version {
                    println!("Package {}@{} is already installed.", name, pkg.version);
                    return Ok(());
                } else {
                    println!("Updating {} from {} to {} ...", name, existing.version, pkg.version);
                    fs::remove_dir_all(&install_dir)
                        .map_err(|e| format!("Failed to remove old version: {}", e))?;
                }
            }
        }

        let zip_path = self.download_package_to_file(&pkg)?;
        self.extract_package_from_file(&zip_path, &install_dir)?;
        let _ = fs::remove_file(&zip_path);
        self.install_dependencies(&install_dir, &pkg.dependencies)?;
        self.update_lock(&name, &pkg)?;

        println!("Successfully installed {}@{}", name, pkg.version);
        Ok(())
    }

    pub fn uninstall(&self, name: &str) -> Result<(), String> {
        let install_dir = self.libs_dir.join(name);
        if !install_dir.exists() {
            return Err(format!("Package '{}' is not installed.", name));
        }

        let dependents = self.find_dependents(name)?;
        if !dependents.is_empty() {
            return Err(format!(
                "Cannot uninstall '{}' because it is required by: {}",
                name,
                dependents.join(", ")
            ));
        }

        fs::remove_dir_all(&install_dir)
            .map_err(|e| format!("Failed to remove package: {}", e))?;

        let mut lock = self.load_lock()?;
        lock.packages.remove(name);
        self.save_lock(&lock)?;

        println!("Uninstalled {}", name);
        Ok(())
    }

    pub fn list(&self) -> Result<(), String> {
        let entries = fs::read_dir(&self.libs_dir)
            .map_err(|e| format!("Failed to read libs dir: {}", e))?;

        println!("Installed packages (global):");
        println!("{}", "-".repeat(50));

        let mut packages = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
            let path = entry.path();
            if path.is_dir() {
                if let Ok(idx) = self.load_package_index(&path) {
                    packages.push((idx.name, idx.version));
                }
            }
        }

        if packages.is_empty() {
            println!("  No packages installed.");
        } else {
            packages.sort_by(|a, b| a.0.cmp(&b.0));
            for (name, version) in packages {
                println!("  {}@{}", name, version);
            }
        }

        Ok(())
    }

    pub fn search(&self, query: &str) -> Result<(), String> {
        let db = self.fetch_database(false)?;
        println!("Searching for '{}' ...", query);
        println!("{}", "-".repeat(50));

        let mut found = false;
        for pkg in db {
            if pkg.name.contains(query) || pkg.description.to_lowercase().contains(&query.to_lowercase()) {
                found = true;
                println!("  {}@{} – {}", pkg.name, pkg.version, pkg.description);
                if !pkg.available_versions.is_empty() {
                    println!("    Available versions: {}", pkg.available_versions.join(", "));
                }
                if !pkg.dependencies.is_empty() {
                    let deps: Vec<String> = pkg.dependencies.keys().cloned().collect();
                    println!("    Dependencies: {}", deps.join(", "));
                }
                if let Some(url) = pkg.repo_url {
                    println!("    Repo: {}", url);
                }
                println!();
            }
        }

        if !found {
            println!("  No packages found matching '{}'", query);
        }

        Ok(())
    }

    pub fn update(&self, pkg_spec: Option<&str>, force_refresh: bool) -> Result<(), String> {
        let db = self.fetch_database(force_refresh)?;
        let mut lock = self.load_lock()?;

        match pkg_spec {
            Some(spec) => {
                let (name, _) = self.parse_spec(spec);
                if let Some(pkg) = self.find_package(&db, &name, "latest").ok() {
                    let install_dir = self.libs_dir.join(&name);
                    if install_dir.exists() {
                        let current = self.load_package_index(&install_dir)?;
                        if current.version != pkg.version {
                            println!("Updating {} from {} to {} ...", name, current.version, pkg.version);
                            self.install(&format!("{}@{}", name, pkg.version))?;
                        } else {
                            println!("{}@{} is already up to date.", name, current.version);
                        }
                    } else {
                        println!("Installing {}@{} ...", name, pkg.version);
                        self.install(&format!("{}@{}", name, pkg.version))?;
                    }
                } else {
                    return Err(format!("Package '{}' not found in database.", name));
                }
            }
            None => {
                let packages: Vec<String> = lock.packages.keys().cloned().collect();
                for name in packages {
                    if let Some(pkg) = self.find_package(&db, &name, "latest").ok() {
                        let install_dir = self.libs_dir.join(&name);
                        if install_dir.exists() {
                            let current = self.load_package_index(&install_dir)?;
                            if current.version != pkg.version {
                                println!("Updating {} from {} to {} ...", name, current.version, pkg.version);
                                self.install(&format!("{}@{}", name, pkg.version))?;
                            }
                        }
                    }
                }
                println!("All packages are up to date.");
            }
        }

        Ok(())
    }

    pub fn update_database(&self) -> Result<(), String> {
        println!("Updating package database ...");
        
        let temp_db = self.cache_dir.join("database.json.tmp");
        self.http_download_to_file(&self.database_url, &temp_db)?;
        
        let content = fs::read_to_string(&temp_db)
            .map_err(|e| format!("Failed to read downloaded database: {}", e))?;
        
        // Verify it's valid JSON
        let _ = self.parse_database_json(&content)?;
        
        // Overwrite the cached database
        fs::write(&self.database_file, &content)
            .map_err(|e| format!("Failed to save database: {}", e))?;
        
        let _ = fs::remove_file(&temp_db);
        
        println!("Database updated successfully.");
        Ok(())
    }


    fn parse_spec(&self, spec: &str) -> (String, String) {
        if spec.contains('@') {
            let parts: Vec<&str> = spec.split('@').collect();
            (parts[0].to_string(), parts[1].to_string())
        } else {
            (spec.to_string(), "latest".to_string())
        }
    }

    fn fetch_database(&self, force_refresh: bool) -> Result<Vec<PackageIndex>, String> {
        if !force_refresh && self.database_file.exists() {
            let content = fs::read_to_string(&self.database_file)
                .map_err(|e| format!("Failed to read cached database: {}", e))?;
            if let Ok(db) = self.parse_database_json(&content) {
                return Ok(db);
            }
        }

        println!("Downloading package database ...");
        
        let temp_db = self.cache_dir.join("database.json.tmp");
        self.http_download_to_file(&self.database_url, &temp_db)?;
        
        let content = fs::read_to_string(&temp_db)
            .map_err(|e| format!("Failed to read downloaded database: {}", e))?;
        
        let db = self.parse_database_json(&content)?;

        fs::write(&self.database_file, &content)
            .map_err(|e| format!("Failed to cache database: {}", e))?;
        
        let _ = fs::remove_file(&temp_db);

        Ok(db)
    }

    fn parse_database_json(&self, json: &str) -> Result<Vec<PackageIndex>, String> {
        let mut packages = Vec::new();
        let mut chars = json.chars().peekable();
        
        while let Some(c) = chars.next() {
            if c == '{' {
                let obj_str = self.extract_object(&mut chars)?;
                if let Some(pkg) = self.parse_package_object(&obj_str) {
                    packages.push(pkg);
                }
            }
        }

        Ok(packages)
    }

    fn extract_object(&self, chars: &mut std::iter::Peekable<std::str::Chars>) -> Result<String, String> {
        let mut depth = 1;
        let mut result = String::new();
        result.push('{');
        while let Some(c) = chars.next() {
            result.push(c);
            if c == '{' { depth += 1; }
            else if c == '}' { depth -= 1; }
            if depth == 0 { break; }
        }
        if depth != 0 {
            return Err("Malformed JSON".to_string());
        }
        Ok(result)
    }

    fn parse_package_object(&self, obj: &str) -> Option<PackageIndex> {
        let mut name = String::new();
        let mut version = String::new();
        let mut available_versions = Vec::new();
        let mut description = String::new();
        let mut main = String::new();
        let mut author = None;
        let mut license = None;
        let mut dependencies = HashMap::new();
        let mut repo_url = None;

        if let Some(val) = self.extract_string_field(obj, "name") {
            name = val;
        }
        if let Some(val) = self.extract_string_field(obj, "version") {
            version = val;
        }
        if let Some(versions_str) = self.extract_array_field(obj, "available-versions") {
            available_versions = self.parse_string_array(&versions_str);
        }
        if let Some(val) = self.extract_string_field(obj, "description") {
            description = val;
        }
        if let Some(val) = self.extract_string_field(obj, "main") {
            main = val;
        }
        if let Some(val) = self.extract_string_field(obj, "author") {
            author = Some(val);
        }
        if let Some(val) = self.extract_string_field(obj, "license") {
            license = Some(val);
        }
        if let Some(val) = self.extract_nested_string_field(obj, "repository", "url") {
            repo_url = Some(val);
        }

        if name.is_empty() || version.is_empty() {
            return None;
        }

        if let Some(dep_str) = self.extract_object_field(obj, "dependencies") {
            let mut dep_chars = dep_str.chars().peekable();
            while let Some(c) = dep_chars.next() {
                if c == '"' {
                    let key = self.read_string_until(&mut dep_chars, '"').unwrap_or_default();
                    while let Some(&ch) = dep_chars.peek() {
                        if ch == ':' { dep_chars.next(); break; }
                        dep_chars.next();
                    }
                    let val = self.read_string_until(&mut dep_chars, '"').unwrap_or_default();
                    if !key.is_empty() {
                        dependencies.insert(key, val);
                    }
                }
            }
        }

        Some(PackageIndex {
            name,
            version,
            available_versions,
            description,
            main,
            author,
            license,
            dependencies,
            repo_url,
        })
    }

    fn extract_string_field(&self, obj: &str, key: &str) -> Option<String> {
        let pattern = format!("\"{}\"", key);
        if let Some(start) = obj.find(&pattern) {
            let after_key = &obj[start + pattern.len()..];
            if let Some(colon) = after_key.find(':') {
                let after_colon = after_key[colon + 1..].trim();
                if after_colon.starts_with('"') {
                    let end = after_colon[1..].find('"')? + 1;
                    return Some(after_colon[1..end].to_string());
                } else {
                    let end = after_colon.find(|c: char| c == ',' || c == '}').unwrap_or(after_colon.len());
                    return Some(after_colon[..end].trim().to_string());
                }
            }
        }
        None
    }

    fn extract_nested_string_field(&self, obj: &str, parent: &str, child: &str) -> Option<String> {
        let pattern = format!("\"{}\"", parent);
        if let Some(start) = obj.find(&pattern) {
            let after = &obj[start + pattern.len()..];
            if let Some(brace) = after.find('{') {
                let mut depth = 0;
                let mut inner = String::new();
                let mut chars = after[brace..].chars();
                while let Some(c) = chars.next() {
                    inner.push(c);
                    if c == '{' { depth += 1; }
                    else if c == '}' { depth -= 1; }
                    if depth == 0 { break; }
                }
                return self.extract_string_field(&inner, child);
            }
        }
        None
    }

    fn extract_object_field(&self, obj: &str, key: &str) -> Option<String> {
        let pattern = format!("\"{}\"", key);
        if let Some(start) = obj.find(&pattern) {
            let after = &obj[start + pattern.len()..];
            if let Some(colon) = after.find(':') {
                let rest = &after[colon + 1..].trim();
                if rest.starts_with('{') {
                    let mut depth = 0;
                    let mut result = String::new();
                    let mut chars = rest.chars();
                    while let Some(c) = chars.next() {
                        result.push(c);
                        if c == '{' { depth += 1; }
                        else if c == '}' { depth -= 1; }
                        if depth == 0 { break; }
                    }
                    if depth == 0 {
                        return Some(result);
                    }
                }
            }
        }
        None
    }

    fn extract_array_field(&self, obj: &str, key: &str) -> Option<String> {
        let pattern = format!("\"{}\"", key);
        if let Some(start) = obj.find(&pattern) {
            let after = &obj[start + pattern.len()..];
            if let Some(colon) = after.find(':') {
                let rest = &after[colon + 1..].trim();
                if rest.starts_with('[') {
                    let mut depth = 0;
                    let mut result = String::new();
                    let mut chars = rest.chars();
                    while let Some(c) = chars.next() {
                        result.push(c);
                        if c == '[' { depth += 1; }
                        else if c == ']' { depth -= 1; }
                        if depth == 0 { break; }
                    }
                    if depth == 0 {
                        return Some(result);
                    }
                }
            }
        }
        None
    }

    fn parse_string_array(&self, array_str: &str) -> Vec<String> {
        let mut result = Vec::new();
        let mut chars = array_str.chars().peekable();
        
        while let Some(c) = chars.next() {
            if c == '"' {
                let val = self.read_string_until(&mut chars, '"').unwrap_or_default();
                if !val.is_empty() {
                    result.push(val);
                }
            }
        }
        
        result
    }

    fn read_string_until(&self, chars: &mut std::iter::Peekable<std::str::Chars>, until: char) -> Option<String> {
        let mut result = String::new();
        while let Some(c) = chars.next() {
            if c == until {
                return Some(result);
            } else {
                result.push(c);
            }
        }
        None
    }

    fn find_package(&self, db: &[PackageIndex], name: &str, version: &str) -> Result<PackageIndex, String> {
        let matches: Vec<PackageIndex> = db.iter()
            .filter(|p| p.name == name)
            .cloned()
            .collect();

        if matches.is_empty() {
            return Err(format!("Package '{}' not found in database.", name));
        }

        if version == "latest" {
            let mut sorted = matches;
            sorted.sort_by(|a, b| b.version.cmp(&a.version));
            return Ok(sorted.remove(0));
        }

        for pkg in &matches {
            if pkg.available_versions.contains(&version.to_string()) {
                for pkg_match in &matches {
                    if pkg_match.version == version {
                        return Ok(pkg_match.clone());
                    }
                }
            }
        }

        for pkg in &matches {
            if pkg.version == version {
                return Ok(pkg.clone());
            }
        }

        if let Some(pkg) = matches.first() {
            if !pkg.available_versions.is_empty() {
                return Err(format!(
                    "Version '{}' not found for package '{}'. Available versions: {}",
                    version,
                    name,
                    pkg.available_versions.join(", ")
                ));
            }
        }

        Err(format!("Version '{}' not found for package '{}'.", version, name))
    }

    fn http_download_to_file(&self, url: &str, output_path: &Path) -> Result<(), String> {
        let c_url = std::ffi::CString::new(url)
            .map_err(|e| format!("Invalid URL: {}", e))?;
        
        let c_output = std::ffi::CString::new(output_path.to_string_lossy().as_bytes())
            .map_err(|e| format!("Invalid path: {}", e))?;
        
        let result = unsafe {
            download_to_file(c_url.as_ptr(), c_output.as_ptr())
        };
        
        if result != 0 {
            return Err(format!("Failed to download: {}", url));
        }
        
        if !output_path.exists() {
            return Err("Download failed: file not created".to_string());
        }
        
        let metadata = fs::metadata(output_path)
            .map_err(|e| format!("Failed to get file metadata: {}", e))?;
        
        if metadata.len() == 0 {
            return Err("Download failed: file is empty".to_string());
        }
        
        Ok(())
    }

    fn download_package_to_file(&self, pkg: &PackageIndex) -> Result<PathBuf, String> {
        let repo_url = pkg.repo_url.as_ref().ok_or_else(|| {
            format!("Package '{}' has no repository URL.", pkg.name)
        })?;

        let url = format!(
            "{}/releases/download/{}/{}-{}.zip",
            repo_url.trim_end_matches('/'),
            pkg.version,
            pkg.name,
            pkg.version
        );

        let zip_path = self.cache_dir.join(format!("{}-{}.zip", pkg.name, pkg.version));
        
        println!("  Downloading from: {}", url);
        println!("  Saving to: {}", zip_path.display());
        
        self.http_download_to_file(&url, &zip_path)?;
        
        let metadata = fs::metadata(&zip_path)
            .map_err(|e| format!("Failed to get file metadata: {}", e))?;
        
        println!("  Downloaded {} bytes", metadata.len());
        
        Ok(zip_path)
    }

    fn extract_package_from_file(&self, zip_path: &Path, install_dir: &Path) -> Result<(), String> {
        fs::create_dir_all(install_dir)
            .map_err(|e| format!("Failed to create install dir: {}", e))?;
        
        let c_zip = std::ffi::CString::new(zip_path.to_string_lossy().as_bytes())
            .map_err(|e| format!("Invalid zip path: {}", e))?;
        
        let c_dest = std::ffi::CString::new(install_dir.to_string_lossy().as_bytes())
            .map_err(|e| format!("Invalid dest path: {}", e))?;
        
        println!("  Extracting to: {}", install_dir.display());
        
        let result = unsafe {
            unzip_file(c_zip.as_ptr(), c_dest.as_ptr())
        };
        
        if result != 0 {
            return Err("Failed to extract package".to_string());
        }
        
        Ok(())
    }

    fn install_dependencies(&self, package_dir: &Path, deps: &HashMap<String, String>) -> Result<(), String> {
        if deps.is_empty() {
            return Ok(());
        }

        println!("  Installing dependencies ...");
        for (dep_name, dep_version) in deps {
            let dep_spec = format!("{}@{}", dep_name, dep_version);
            println!("    -> {}", dep_spec);

            let dep_dir = self.libs_dir.join(&dep_name);
            if dep_dir.exists() {
                if let Ok(idx) = self.load_package_index(&dep_dir) {
                    if self.version_matches(&idx.version, &dep_version) {
                        println!("      Already installed ({}@{})", dep_name, idx.version);
                        continue;
                    }
                }
            }

            self.install(&dep_spec)?;
        }

        Ok(())
    }

    fn version_matches(&self, installed: &str, required: &str) -> bool {
        if required == "latest" {
            return true;
        }
        if required.starts_with('^') || required.starts_with('~') {
            let req_ver = required.trim_start_matches(&['^', '~', '='][..]);
            return installed == req_ver;
        }
        installed == required
    }

    fn load_lock(&self) -> Result<LockFile, String> {
        if self.lock_file.exists() {
            let content = fs::read_to_string(&self.lock_file)
                .map_err(|e| format!("Failed to read lock file: {}", e))?;
            self.parse_lock_json(&content)
        } else {
            Ok(LockFile {
                packages: HashMap::new(),
            })
        }
    }

    fn parse_lock_json(&self, json: &str) -> Result<LockFile, String> {
        let mut packages = HashMap::new();
        if let Some(pkg_obj) = self.extract_object_field(json, "packages") {
            let mut chars = pkg_obj.chars().peekable();
            while let Some(c) = chars.next() {
                if c == '"' {
                    let name = self.read_string_until(&mut chars, '"').unwrap_or_default();
                    while let Some(&ch) = chars.peek() {
                        if ch == ':' { chars.next(); break; }
                        chars.next();
                    }
                    let val_obj = self.extract_object(&mut chars)?;
                    let version = self.extract_string_field(&val_obj, "version").unwrap_or_default();
                    let resolved_url = self.extract_string_field(&val_obj, "resolved_url").unwrap_or_default();
                    let mut deps = Vec::new();
                    if let Some(dep_str) = self.extract_object_field(&val_obj, "dependencies") {
                        let mut dep_chars = dep_str.chars().peekable();
                        while let Some(c) = dep_chars.next() {
                            if c == '"' {
                                let dep = self.read_string_until(&mut dep_chars, '"').unwrap_or_default();
                                if !dep.is_empty() {
                                    deps.push(dep);
                                }
                            }
                        }
                    }
                    packages.insert(name, LockedPackage {
                        version,
                        resolved_url,
                        dependencies: deps,
                    });
                }
            }
        }
        Ok(LockFile { packages })
    }

    fn save_lock(&self, lock: &LockFile) -> Result<(), String> {
        let content = self.serialize_lock(lock);
        fs::write(&self.lock_file, content)
            .map_err(|e| format!("Failed to write lock file: {}", e))
    }

    fn serialize_lock(&self, lock: &LockFile) -> String {
        let mut lines = Vec::new();
        lines.push("{".to_string());
        lines.push("  \"packages\": {".to_string());
        let mut first = true;
        for (name, info) in &lock.packages {
            if !first {
                lines.push(",".to_string());
            }
            first = false;
            lines.push(format!("    \"{}\": {{", name));
            lines.push(format!("      \"version\": \"{}\",", info.version));
            lines.push(format!("      \"resolved_url\": \"{}\",", info.resolved_url));
            lines.push("      \"dependencies\": [".to_string());
            let mut dep_first = true;
            for dep in &info.dependencies {
                if !dep_first {
                    lines.push(",".to_string());
                }
                dep_first = false;
                lines.push(format!("        \"{}\"", dep));
            }
            lines.push("      ]".to_string());
            lines.push("    }".to_string());
        }
        lines.push("  }".to_string());
        lines.push("}".to_string());
        lines.join("\n")
    }

    fn update_lock(&self, name: &str, pkg: &PackageIndex) -> Result<(), String> {
        let mut lock = self.load_lock()?;
        let resolved_url = pkg.repo_url.as_ref().map(|url| {
            format!("{}/releases/download/{}/{}-{}.zip",
                url.trim_end_matches('/'),
                pkg.version,
                pkg.name,
                pkg.version
            )
        }).unwrap_or_else(|| "".to_string());

        let locked = LockedPackage {
            version: pkg.version.clone(),
            resolved_url,
            dependencies: pkg.dependencies.keys().cloned().collect(),
        };

        lock.packages.insert(name.to_string(), locked);
        self.save_lock(&lock)
    }

    fn find_dependents(&self, name: &str) -> Result<Vec<String>, String> {
        let lock = self.load_lock()?;
        let mut dependents = Vec::new();
        for (pkg, info) in &lock.packages {
            if info.dependencies.contains(&name.to_string()) {
                dependents.push(pkg.clone());
            }
        }
        Ok(dependents)
    }

    fn load_package_index(&self, package_dir: &Path) -> Result<PackageIndex, String> {
        let index_path = package_dir.join("index.json");
        let content = fs::read_to_string(&index_path)
            .map_err(|e| format!("Failed to read index.json: {}", e))?;

        let name = self.extract_string_field(&content, "name").unwrap_or_default();
        let version = self.extract_string_field(&content, "version").unwrap_or_default();
        let description = self.extract_string_field(&content, "description").unwrap_or_default();
        let main = self.extract_string_field(&content, "main").unwrap_or_default();
        let author = self.extract_string_field(&content, "author");
        let license = self.extract_string_field(&content, "license");
        let mut dependencies = HashMap::new();
        
        if let Some(dep_obj) = self.extract_object_field(&content, "dependencies") {
            let mut chars = dep_obj.chars().peekable();
            while let Some(c) = chars.next() {
                if c == '"' {
                    let key = self.read_string_until(&mut chars, '"').unwrap_or_default();
                    while let Some(&ch) = chars.peek() {
                        if ch == ':' { chars.next(); break; }
                        chars.next();
                    }
                    let val = self.read_string_until(&mut chars, '"').unwrap_or_default();
                    if !key.is_empty() {
                        dependencies.insert(key, val);
                    }
                }
            }
        }

        Ok(PackageIndex {
            name,
            version,
            available_versions: Vec::new(),
            description,
            main,
            author,
            license,
            dependencies,
            repo_url: None,
        })
    }
}

pub fn handle_install(spec: &str) {
    let pacman = match Pacman::new() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };
    if let Err(e) = pacman.install(spec) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

pub fn handle_uninstall(name: &str) {
    let pacman = match Pacman::new() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };
    if let Err(e) = pacman.uninstall(name) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

pub fn handle_list() {
    let pacman = match Pacman::new() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };
    if let Err(e) = pacman.list() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

pub fn handle_search(query: &str) {
    let pacman = match Pacman::new() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };
    if let Err(e) = pacman.search(query) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

pub fn handle_update(pkg: Option<&str>, force_refresh: bool) {
    let pacman = match Pacman::new() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };
    if let Err(e) = pacman.update(pkg, force_refresh) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

pub fn handle_update_database() {
    let pacman = match Pacman::new() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };
    if let Err(e) = pacman.update_database() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}