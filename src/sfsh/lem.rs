use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn lem_dir() -> PathBuf {
    let home = env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".lem")
}

pub fn envs_dir() -> PathBuf {
    lem_dir().join("envs")
}

fn is_nixos() -> bool {
    Path::new("/run/current-system").exists()
}

fn is_gentoo() -> bool {
    Path::new("/etc/gentoo-release").exists()
}

fn env_path(name: &str) -> PathBuf {
    envs_dir().join(name)
}

fn packages_file(name: &str) -> PathBuf {
    env_path(name).join("packages")
}

fn cache_file(name: &str) -> PathBuf {
    env_path(name).join("cache")
}

fn vars_file(name: &str) -> PathBuf {
    env_path(name).join("vars")
}

pub fn list_envs() -> Vec<String> {
    let dir = envs_dir();
    if !dir.exists() {
        return Vec::new();
    }

    let mut envs = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    envs.push(name.to_string());
                }
            }
        }
    }
    envs.sort();
    envs
}

pub fn create_env(name: &str, packages: &[String]) -> Result<(), String> {
    let path = env_path(name);

    if path.exists() {
        return Err(format!("environment '{}' already exists", name));
    }

    fs::create_dir_all(&path).map_err(|e| format!("cannot create {}: {}", path.display(), e))?;

    let pkgs = packages.join("\n");
    fs::write(packages_file(name), pkgs).map_err(|e| format!("cannot write packages: {}", e))?;

    fs::write(vars_file(name), "").map_err(|e| format!("cannot write vars: {}", e))?;

    println!("created environment '{}'", name);
    println!("enter with: sfsh lem enter {}", name);

    Ok(())
}

pub fn remove_env(name: &str) -> Result<(), String> {
    let path = env_path(name);

    if !path.exists() {
        return Err(format!("environment '{}' not found", name));
    }

    fs::remove_dir_all(&path).map_err(|e| format!("cannot remove {}: {}", path.display(), e))?;

    println!("removed environment '{}'", name);
    Ok(())
}

pub fn add_package(name: &str, package: &str) -> Result<(), String> {
    let path = env_path(name);

    if !path.exists() {
        return Err(format!("environment '{}' not found", name));
    }

    let pkg_file = packages_file(name);
    let mut content = fs::read_to_string(&pkg_file).unwrap_or_default();

    if content.lines().any(|l| l.trim() == package) {
        return Err(format!(
            "package '{}' already in environment '{}'",
            package, name
        ));
    }

    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(package);
    content.push('\n');

    fs::write(&pkg_file, content).map_err(|e| format!("cannot write packages: {}", e))?;

    let cache = cache_file(name);
    if cache.exists() {
        let _ = fs::remove_file(&cache);
    }

    println!("added '{}' to environment '{}'", package, name);
    Ok(())
}

pub fn env_status(name: &str) -> Result<(), String> {
    let path = env_path(name);

    if !path.exists() {
        return Err(format!("environment '{}' not found", name));
    }

    let pkg_file = packages_file(name);
    let packages = fs::read_to_string(&pkg_file).unwrap_or_default();

    println!("environment: {}", name);
    println!("path: {}", path.display());
    println!("packages:");

    for pkg in packages.lines() {
        let pkg = pkg.trim();
        if pkg.is_empty() {
            continue;
        }

        let installed = if is_nixos() {
            check_nix_package(pkg)
        } else if is_gentoo() {
            check_gentoo_package(pkg)
        } else {
            false
        };

        let status = if installed { "installed" } else { "missing" };
        println!("  {} [{}]", pkg, status);
    }

    let cache = cache_file(name);
    if cache.exists() {
        println!("cache: valid");
    } else {
        println!("cache: none");
    }

    Ok(())
}

fn check_nix_package(pkg: &str) -> bool {
    Command::new("nix-store")
        .args(["--query", "--exists"])
        .arg(format!("/nix/store/*-{}", pkg))
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn check_gentoo_package(pkg: &str) -> bool {
    let pkg_db = Path::new("/var/db/pkg");
    if !pkg_db.exists() {
        return false;
    }

    let parts: Vec<&str> = pkg.split('/').collect();
    if parts.len() == 2 {
        let cat_dir = pkg_db.join(parts[0]);
        if cat_dir.exists() {
            if let Ok(entries) = fs::read_dir(&cat_dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with(parts[1]) {
                        return true;
                    }
                }
            }
        }
    }

    false
}

fn read_packages(name: &str) -> Vec<String> {
    let pkg_file = packages_file(name);
    fs::read_to_string(&pkg_file)
        .unwrap_or_default()
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

fn read_vars(name: &str) -> HashMap<String, String> {
    let vars_file = vars_file(name);
    let mut vars = HashMap::new();

    if let Ok(content) = fs::read_to_string(&vars_file) {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                vars.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
    }

    vars
}

fn build_env_vars(name: &str) -> Result<HashMap<String, String>, String> {
    let cache = cache_file(name);

    if cache.exists() {
        if let Ok(content) = fs::read_to_string(&cache) {
            let mut vars = HashMap::new();
            for line in content.lines() {
                if let Some((k, v)) = line.split_once('=') {
                    vars.insert(k.to_string(), v.to_string());
                }
            }
            if !vars.is_empty() {
                return Ok(vars);
            }
        }
    }

    let packages = read_packages(name);
    let mut vars = HashMap::new();

    if is_nixos() {
        let nix_packages: Vec<&str> = packages.iter().map(|s| s.as_str()).collect();
        let mut cmd = Command::new("nix-shell");
        for pkg in &nix_packages {
            cmd.arg("-p").arg(pkg);
        }
        cmd.arg("--run").arg("env");

        let output = cmd
            .output()
            .map_err(|e| format!("nix-shell failed: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("nix-shell failed: {}", stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if let Some((k, v)) = line.split_once('=') {
                if k == "PATH"
                    || k == "LD_LIBRARY_PATH"
                    || k == "PKG_CONFIG_PATH"
                    || k == "MANPATH"
                    || k == "NIX_BUILD_TOP"
                    || k.starts_with("NIX_")
                {
                    vars.insert(k.to_string(), v.to_string());
                }
            }
        }

        let cache_content: Vec<String> = vars.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
        let _ = fs::write(&cache, cache_content.join("\n"));
    } else if is_gentoo() {
        let path = env::var("PATH").unwrap_or_default();
        vars.insert("PATH".to_string(), path);

        for pkg in &packages {
            if !check_gentoo_package(pkg) {
                eprintln!(
                    "warning: package '{}' not installed, run: emerge {}",
                    pkg, pkg
                );
            }
        }
    } else {
        let path = env::var("PATH").unwrap_or_default();
        vars.insert("PATH".to_string(), path);
    }

    let user_vars = read_vars(name);
    for (k, v) in user_vars {
        vars.insert(k, v);
    }

    vars.insert("LEM_ENV".to_string(), name.to_string());

    Ok(vars)
}

pub fn enter_env(name: &str) -> Result<i32, String> {
    let path = env_path(name);

    if !path.exists() {
        return Err(format!("environment '{}' not found", name));
    }

    let vars = build_env_vars(name)?;

    let current_exe = env::current_exe().map_err(|e| format!("cannot find current exe: {}", e))?;

    let mut cmd = Command::new(&current_exe);

    for (k, v) in &vars {
        cmd.env(k, v);
    }

    cmd.env("LEM_ENV", name);
    cmd.stdin(std::process::Stdio::inherit());
    cmd.stdout(std::process::Stdio::inherit());
    cmd.stderr(std::process::Stdio::inherit());

    let status = cmd
        .status()
        .map_err(|e| format!("failed to start sfsh: {}", e))?;

    Ok(status.code().unwrap_or(0))
}

pub fn exec_in_env(name: &str, command: &[String]) -> Result<i32, String> {
    let path = env_path(name);

    if !path.exists() {
        return Err(format!("environment '{}' not found", name));
    }

    if command.is_empty() {
        return Err("no command specified".to_string());
    }

    let vars = build_env_vars(name)?;

    let mut cmd = Command::new(&command[0]);
    cmd.args(&command[1..]);

    for (k, v) in &vars {
        cmd.env(k, v);
    }

    cmd.env("LEM_ENV", name);

    let status = cmd
        .status()
        .map_err(|e| format!("failed to execute: {}", e))?;

    Ok(status.code().unwrap_or(0))
}

pub fn lem_main(args: &[String]) -> Result<i32, String> {
    if args.is_empty() {
        return Err("usage: sfsh lem <create|enter|list|remove|add|status|exec>".to_string());
    }

    match args[0].as_str() {
        "create" => {
            if args.len() < 2 {
                return Err("usage: sfsh lem create <name> [packages...]".to_string());
            }
            create_env(&args[1], &args[2..])?;
            Ok(0)
        }
        "enter" => {
            if args.len() < 2 {
                return Err("usage: sfsh lem enter <name>".to_string());
            }
            enter_env(&args[1])
        }
        "list" => {
            let envs = list_envs();
            if envs.is_empty() {
                println!("no environments");
            } else {
                for env in envs {
                    println!("{}", env);
                }
            }
            Ok(0)
        }
        "remove" => {
            if args.len() < 2 {
                return Err("usage: sfsh lem remove <name>".to_string());
            }
            remove_env(&args[1])?;
            Ok(0)
        }
        "add" => {
            if args.len() < 3 {
                return Err("usage: sfsh lem add <name> <package>".to_string());
            }
            add_package(&args[1], &args[2])?;
            Ok(0)
        }
        "status" => {
            if args.len() < 2 {
                return Err("usage: sfsh lem status <name>".to_string());
            }
            env_status(&args[1])?;
            Ok(0)
        }
        "exec" => {
            if args.len() < 3 {
                return Err("usage: sfsh lem exec <name> <command...>".to_string());
            }
            exec_in_env(&args[1], &args[2..])
        }
        _ => Err(format!("unknown lem command: {}", args[0])),
    }
}
