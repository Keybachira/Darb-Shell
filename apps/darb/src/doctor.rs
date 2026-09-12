//! `darb doctor` — read-only environment check (Arquitetura §58).
//! Never mutates anything: every check is a probe, and secret values
//! are never printed (only whether a key is set).
//!
//! Labels come from the locales, so the report is readable in the same
//! language as the TUI (§15 Contribuição).

use std::path::PathBuf;

use darb_core::config::DarbConfig;
use darb_core::t;

struct Check {
    /// i18n key for the check name.
    name: &'static str,
    ok: bool,
    detail: String,
}

pub fn run() -> i32 {
    crate::tui_loop::init_i18n();
    let config_path = crate::tui_loop::config_path();
    let mut checks = Vec::new();

    let (config, config_check) = load_config(&config_path);
    checks.push(config_check);

    let provider = config.provider.name.clone();
    checks.push(provider_check(&provider));

    checks.push(tool_check("Git", "git", &["--version"]));
    checks.push(tool_check("ripgrep", "rg", &["--version"]));

    let darb_dir = std::env::current_dir()
        .map(|cwd| cwd.join(".darb"))
        .unwrap_or_else(|_| PathBuf::from(".darb"));
    checks.push(Check {
        name: "doctor.project",
        ok: true,
        detail: if darb_dir.join("config.toml").exists() {
            t!("doctor.initialized", path = darb_dir.display().to_string())
        } else {
            t!("doctor.not_initialized")
        },
    });

    println!("{}\n", t!("doctor.title"));
    let mut failed = 0;
    for check in &checks {
        // `ok: false` is reserved for hard failures; degraded states
        // (missing key, uninitialized project) stay informational.
        let mark = if check.ok { "✓" } else { "✗" };
        if !check.ok {
            failed += 1;
        }
        println!("{mark} {}: {}", t!(check.name), check.detail);
    }
    if failed == 0 {
        println!("\n{}", t!("doctor.all_passed"));
        0
    } else {
        println!("\n{}", t!("doctor.failed", count = failed));
        1
    }
}

/// Read + parse the config. A malformed file is a real failure (the run
/// would silently fall back to defaults); a missing one is reported with
/// its path so the user can see which file was expected.
fn load_config(path: &std::path::Path) -> (DarbConfig, Check) {
    match std::fs::read_to_string(path) {
        Ok(text) => match darb_core::config::load_from_str(&text) {
            Ok(config) => (
                config,
                Check {
                    name: "doctor.config",
                    ok: true,
                    detail: path.display().to_string(),
                },
            ),
            Err(e) => (
                DarbConfig::default(),
                Check {
                    name: "doctor.config",
                    ok: false,
                    detail: e.to_string(),
                },
            ),
        },
        Err(_) => (
            DarbConfig::default(),
            Check {
                name: "doctor.config",
                ok: false,
                detail: t!("doctor.config_missing", path = path.display().to_string()),
            },
        ),
    }
}

/// Providers that exist today (only OpenAI ships an adapter) get a key
/// check; the rest say so instead of implying they work.
fn provider_check(provider: &str) -> Check {
    let variables: &[&str] = match provider {
        "openai" => &["DARB_OPENAI_API_KEY", "OPENAI_API_KEY"],
        "anthropic" => &["DARB_ANTHROPIC_API_KEY", "ANTHROPIC_API_KEY"],
        _ => &[],
    };
    if variables.is_empty() {
        return Check {
            name: "doctor.provider",
            // Not a failure: the local tools work without any provider.
            ok: true,
            detail: format!("{provider} ({})", t!("doctor.provider_unimplemented")),
        };
    }
    let key_set = variables
        .iter()
        .any(|var| std::env::var_os(var).is_some_and(|value| !value.is_empty()));
    Check {
        name: "doctor.provider",
        ok: true,
        detail: format!(
            "{provider} ({})",
            if key_set {
                t!("doctor.key_set")
            } else {
                t!("doctor.key_missing")
            }
        ),
    }
}

fn tool_check(name: &'static str, binary: &str, args: &[&str]) -> Check {
    match std::process::Command::new(binary).args(args).output() {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            Check {
                name,
                ok: true,
                detail: version,
            }
        }
        _ => Check {
            name,
            ok: false,
            detail: t!("doctor.tool_missing", binary = binary),
        },
    }
}
