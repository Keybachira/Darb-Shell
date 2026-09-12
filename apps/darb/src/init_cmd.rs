//! `darb init` — create `.darb/config.toml` for this project.
//! Never overwrites: an existing file means the project is initialized.

use darb_core::t;

const TEMPLATE: &str = r#"# Darb project configuration (TOML).
# Secrets don't belong here — use DARB_<PROVIDER>_API_KEY env vars.

[project]
language = "pt"

[provider]
# name = "openai"
# model = "gpt-4o-mini"

[permissions]
read = "allow"
edit = "ask"
shell = "ask"
delete = "ask"
git_push = "deny"
"#;

pub fn run() -> i32 {
    crate::tui_loop::init_i18n();
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("{}", t!("init.error", reason = e.to_string()));
            return 1;
        }
    };
    let dir = cwd.join(".darb");
    let file = dir.join("config.toml");
    if file.exists() {
        println!("{}", t!("init.exists", path = file.display().to_string()));
        return 0;
    }
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!(
            "{}",
            t!(
                "init.error",
                reason = format!("could not create {}: {e}", dir.display())
            )
        );
        return 1;
    }
    match std::fs::write(&file, TEMPLATE) {
        Ok(()) => {
            println!("{}", t!("init.created", path = file.display().to_string()));
            0
        }
        Err(e) => {
            eprintln!(
                "{}",
                t!(
                    "init.error",
                    reason = format!("could not write {}: {e}", file.display())
                )
            );
            1
        }
    }
}
