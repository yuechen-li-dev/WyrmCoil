#![allow(non_snake_case)]
use std::collections::HashSet;
use std::path::PathBuf;

use wyrmcoil::wyrmfmt::CheckRustCasing;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match Run(args) {
        Ok(code) => std::process::exit(code),
        Err(message) => {
            eprintln!("{message}");
            PrintUsage();
            std::process::exit(2);
        }
    }
}

fn Run(args: Vec<String>) -> Result<i32, String> {
    if args.is_empty() {
        return Err("missing command".to_string());
    }
    if args[0] != "check" {
        return Err(format!("unknown command '{}'", args[0]));
    }

    let mut index = 1;
    if index < args.len() && args[index] == "--lang" {
        if index + 1 >= args.len() {
            return Err("--lang requires a value".to_string());
        }
        let language = args[index + 1].as_str();
        if language != "rust" {
            return Err(format!("unsupported language '{language}'"));
        }
        index += 2;
    }

    if index >= args.len() {
        return Err("check requires at least one path".to_string());
    }

    let paths: Vec<PathBuf> = args[index..].iter().map(PathBuf::from).collect();
    let baseline = LoadBaseline("tools/wyrmfmt-baseline.txt")?;
    let violations = CheckRustCasing(&paths)?;

    let mut remaining = Vec::new();
    for violation in violations {
        let line = FormatViolation(&violation);
        if !baseline.contains(&line) {
            remaining.push(line);
        }
    }

    for line in &remaining {
        eprintln!("{line}");
    }

    Ok(if remaining.is_empty() { 0 } else { 1 })
}

fn FormatViolation(violation: &wyrmcoil::wyrmfmt::Violation) -> String {
    format!(
        "{}:{}: function '{}' should be PascalCase; suggested '{}'",
        violation.Path.display(),
        violation.Line,
        violation.FunctionName,
        violation.SuggestedName
    )
}

fn LoadBaseline(path: &str) -> Result<HashSet<String>, String> {
    let baseline_path = PathBuf::from(path);
    if !baseline_path.exists() {
        return Ok(HashSet::new());
    }

    let contents = std::fs::read_to_string(&baseline_path).map_err(|err| {
        format!(
            "failed to read baseline '{}': {err}",
            baseline_path.display()
        )
    })?;
    let lines = contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    Ok(lines)
}

fn PrintUsage() {
    eprintln!("usage: wyrmfmt check [--lang rust] <path> [path ...]");
    eprintln!("notes: M44 check-mode only; auto-fix/rewrite not implemented");
}
