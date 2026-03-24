use agentrail_core::error::Result;
use agentrail_store::saga;
use std::path::Path;

const CLAUDE_MD_TEMPLATE: &str = r#"# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## CRITICAL: AgentRail Session Protocol (MUST follow exactly)

This project uses AgentRail. Every session follows this exact sequence:

### 1. START (do this FIRST, before anything else)
```bash
agentrail next
```
Read the output carefully. It tells you your current step, prompt, skill docs, and past trajectories.

### 2. BEGIN (immediately after reading the next output)
```bash
agentrail begin
```

### 3. WORK (do what the step prompt says)
Do NOT ask the user "want me to proceed?" or "shall I start?". The step prompt IS your instruction. Execute it.

### 4. COMMIT (after the work is done)
Commit your code changes with git.

### 5. COMPLETE (LAST thing, after committing)
```bash
agentrail complete --summary "what you accomplished" \
  --reward 1 \
  --actions "tools and approach used"
```
If the step failed: `--reward -1 --failure-mode "what went wrong"`
If the saga is finished: add `--done`

### 6. STOP (after complete, DO NOT continue working)
Do NOT make any further code changes after running agentrail complete.
Any changes after complete are untracked and invisible to the next session.
If you see more work to do, it belongs in the NEXT step, not this session.

Do NOT skip any of these steps. The next session depends on your trajectory recording.
"#;

pub fn run(saga_path: &Path, name: &str, plan_raw: &str, domain: Option<&str>) -> Result<()> {
    let plan = agentrail_core::read_input(plan_raw)?;

    // 1. Initialize the saga
    if saga::saga_exists(saga_path) {
        println!("Saga already exists. Skipping init.");
    } else {
        saga::init_saga(saga_path, name, &plan)?;
        println!("Initialized saga '{}'.", name);
    }

    // 2. Create CLAUDE.md if it doesn't exist
    let claude_md_path = saga_path.join("CLAUDE.md");
    if claude_md_path.exists() {
        println!("CLAUDE.md already exists. Skipping.");
        println!("  Tip: make sure it contains the AgentRail session protocol.");
    } else {
        std::fs::write(&claude_md_path, CLAUDE_MD_TEMPLATE)?;
        println!("Created CLAUDE.md with AgentRail session protocol.");
    }

    // 3. Register domain if specified
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
                println!("Added domain '{}' to domains.toml.", resolved);
            }
        } else {
            std::fs::write(&domains_toml, &entry)?;
            println!("Created domains.toml with domain '{}'.", resolved);
        }
    }

    println!();
    println!("Setup complete! Next steps:");
    println!();
    println!("  1. Create the first step:");
    println!("     agentrail complete --summary \"Project initialized\" \\");
    println!("       --next-slug <first-step> \\");
    println!("       --next-prompt \"Instructions for first step\" \\");
    println!("       --next-task-type <task-type>");
    println!();
    println!("  2. Start Claude Code:");
    println!("     claude \"go\"");
    println!();

    Ok(())
}
