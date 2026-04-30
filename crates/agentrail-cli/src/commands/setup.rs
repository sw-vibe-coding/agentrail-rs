use agentrail_core::error::Result;
use agentrail_store::{instructions, saga};
use std::path::Path;

/// Minimal CLAUDE.md scaffold — just an H1 and a project-notes section.
/// The agentrail session protocol, metadata discipline, push and recovery
/// rules are NOT inlined here: they are stamped between the briefing
/// markers by `instructions::apply()`, so they update centrally on every
/// `agentrail instructions apply` rather than going stale per-project.
fn claude_md_stub(name: &str) -> String {
    format!(
        "# CLAUDE.md — {name}\n\
         \n\
         This file gives Claude Code (and other agentrail-aware agents) the \
         rules for this project. The block below the H1 marked as \
         `agentrail:global` is auto-managed by `agentrail instructions apply` \
         — content outside those markers is preserved verbatim.\n\
         \n\
         ## Project-specific notes\n\
         \n\
         *(add project-specific build commands, architecture, and conventions \
         here)*\n",
    )
}

/// Minimal AGENTS.md — a thin pointer for codex / opencode / GLM / Gemini
/// that look for `AGENTS.md` instead of `CLAUDE.md`. Local content is kept
/// minimal so the briefing block (stamped between markers) is the single
/// source of truth for shared rules; project-specific rules live in
/// CLAUDE.md.
const AGENTS_MD_STUB: &str = "# AGENTS.md\n\
\n\
This project uses **agentrail** for session-based saga/step orchestration.\n\
\n\
**Quick start:** run `agentrail next`, then `agentrail begin`, do the work \
in the step prompt, commit, then `agentrail complete`. Push the branch.\n\
\n\
**For the full set of rules, read [CLAUDE.md](CLAUDE.md).** Project-specific \
rules and architecture notes live there. The briefing block in this file \
matches the one in CLAUDE.md by construction (both are managed by \
`agentrail instructions apply`), so non-Claude agents can rely on it as \
authoritative for shared rules.\n";

pub fn run(saga_path: &Path, name: &str, plan_raw: &str, domain: Option<&str>) -> Result<()> {
    let plan = agentrail_core::read_input(plan_raw)?;

    // 1. Initialize the saga
    if saga::saga_exists(saga_path) {
        println!("Saga already exists. Skipping init.");
    } else {
        saga::init_saga(saga_path, name, &plan)?;
        println!("Initialized saga '{name}'.");
    }

    // 2. Lay down stub CLAUDE.md / AGENTS.md if missing — apply will stamp
    //    the briefing block into them.
    let claude_path = saga_path.join("CLAUDE.md");
    if !claude_path.exists() {
        std::fs::write(&claude_path, claude_md_stub(name))?;
        println!("Created CLAUDE.md stub.");
    } else {
        println!("CLAUDE.md already exists. Briefing block (if any) will be refreshed.");
    }

    let agents_path = saga_path.join("AGENTS.md");
    if !agents_path.exists() {
        std::fs::write(&agents_path, AGENTS_MD_STUB)?;
        println!("Created AGENTS.md pointer stub.");
    } else {
        println!("AGENTS.md already exists. Briefing block (if any) will be refreshed.");
    }

    // 3. Stamp the canonical briefing into both targets.
    match instructions::apply(saga_path) {
        Ok((profile, outcomes)) => {
            println!("Applied briefing profile '{profile}' to {} target(s).", outcomes.len());
        }
        Err(e) => {
            eprintln!("Warning: could not apply briefing: {e}");
            eprintln!("  Run `agentrail instructions apply` manually after fixing.");
        }
    }

    // 4. Register domain if specified
    if let Some(domain_path) = domain {
        let saga_dir = saga::saga_dir(saga_path);
        let domains_toml = saga_dir.join("domains.toml");
        let resolved = if Path::new(domain_path).is_absolute() {
            domain_path.to_string()
        } else {
            std::fs::canonicalize(domain_path)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| domain_path.to_string())
        };

        let entry = format!("[[domain]]\nname = \"{name}\"\npath = \"{resolved}\"\n");
        if domains_toml.exists() {
            let existing = std::fs::read_to_string(&domains_toml)?;
            if existing.contains(&resolved) {
                println!("Domain already registered. Skipping.");
            } else {
                let updated = format!("{existing}\n{entry}");
                std::fs::write(&domains_toml, updated)?;
                println!("Added domain '{resolved}' to domains.toml.");
            }
        } else {
            std::fs::write(&domains_toml, &entry)?;
            println!("Created domains.toml with domain '{resolved}'.");
        }
    }

    println!();
    println!("Setup complete! Next steps:");
    println!();
    println!("  1. Commit the new files:");
    println!("     git add CLAUDE.md AGENTS.md .agentrail/");
    println!("     git commit -m \"chore: bootstrap agentrail saga + briefing\"");
    println!();
    println!("  2. Create the first step:");
    println!("     agentrail complete --summary \"Project initialized\" \\");
    println!("       --next-slug <first-step> \\");
    println!("       --next-prompt \"Instructions for first step\" \\");
    println!("       --next-task-type <task-type>");
    println!();
    println!("  3. Start your agent:");
    println!("     claude \"go\"     # or codex / opencode");
    println!();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn setup_creates_saga_claude_agents_with_briefing() {
        let tmp = tempdir().unwrap();
        run(tmp.path(), "test-project", "Build a thing", None).unwrap();

        // Saga exists
        assert!(saga::saga_exists(tmp.path()));

        // CLAUDE.md exists, has the project name AND the briefing block
        let claude = std::fs::read_to_string(tmp.path().join("CLAUDE.md")).unwrap();
        assert!(claude.contains("# CLAUDE.md — test-project"));
        assert!(claude.contains("agentrail:global:start"));
        assert!(claude.contains("Agentrail metadata discipline"));

        // AGENTS.md is a thin pointer + briefing
        let agents = std::fs::read_to_string(tmp.path().join("AGENTS.md")).unwrap();
        assert!(agents.contains("read [CLAUDE.md]"));
        assert!(agents.contains("agentrail:global:start"));

        // Lock file written
        assert!(
            tmp.path()
                .join(".agentrail/instruction-lock.toml")
                .is_file()
        );
    }

    #[test]
    fn setup_preserves_existing_claude_md_local_content() {
        let tmp = tempdir().unwrap();
        let claude = tmp.path().join("CLAUDE.md");
        std::fs::write(&claude, "# Existing\n\n## Special local rule\n\nKeep me.\n").unwrap();

        run(tmp.path(), "p", "plan", None).unwrap();

        let body = std::fs::read_to_string(&claude).unwrap();
        assert!(body.contains("# Existing"));
        assert!(body.contains("## Special local rule"));
        assert!(body.contains("Keep me."));
        assert!(body.contains("agentrail:global:start"));
    }

    #[test]
    fn setup_is_idempotent() {
        let tmp = tempdir().unwrap();
        run(tmp.path(), "p", "plan", None).unwrap();
        let before = std::fs::read(tmp.path().join("CLAUDE.md")).unwrap();
        // Re-run setup; saga + files exist; briefing apply should be no-op.
        run(tmp.path(), "p", "plan", None).unwrap();
        let after = std::fs::read(tmp.path().join("CLAUDE.md")).unwrap();
        assert_eq!(before, after);
    }
}
