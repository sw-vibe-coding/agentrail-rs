#!/bin/bash
# Demo 3: End-to-end -- use agentrail to actually create a Rust project
# and fix clippy warnings, guided by skills
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
DEMO_DIR=$(mktemp -d)
cd "$DEMO_DIR"

echo "=== Demo: Rust Project Creation Guided by Skills ==="
echo "Working in: $DEMO_DIR"
echo

# Init saga
agentrail init --name my-greeter --plan "Create a Rust greeter CLI"

# Load skills
SAGA_DIR="$DEMO_DIR/.agentrail"
mkdir -p "$SAGA_DIR/skills"
cp "$SCRIPT_DIR/skills/rust-project-init.toml" "$SAGA_DIR/skills/"
cp "$SCRIPT_DIR/skills/clippy-fix.toml" "$SAGA_DIR/skills/"

# Step 1: create the project (guided by rust-project-init skill)
agentrail complete \
    --summary "Saga initialized" \
    --next-slug create-project \
    --next-prompt "Create the greeter Rust project" \
    --next-role production \
    --next-task-type rust-project-init

echo "--- Step 1: Agent sees skill guidance ---"
agentrail next
echo

echo "--- Following the skill procedure: ---"
agentrail begin

# Actually create the Rust project
echo "$ cargo init --name greeter"
cargo init --name greeter .
echo

# Follow skill step: set edition 2024
echo "Setting edition = '2024' in Cargo.toml..."
sed -i '' 's/edition = "2021"/edition = "2024"/' Cargo.toml
echo

echo "$ grep edition Cargo.toml"
grep edition Cargo.toml
echo

# Intentionally write code with a clippy warning (derivable Default impl)
cat > src/main.rs << 'RUST'
use std::fmt;

#[derive(Debug)]
enum Greeting {
    Hello,
    Hi,
    Hey,
}

impl Default for Greeting {
    fn default() -> Self {
        Greeting::Hello
    }
}

impl fmt::Display for Greeting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Greeting::Hello => write!(f, "Hello"),
            Greeting::Hi => write!(f, "Hi"),
            Greeting::Hey => write!(f, "Hey"),
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let greeting = match args.get(1).map(|s| s.as_str()) {
        Some("hi") => Greeting::Hi,
        Some("hey") => Greeting::Hey,
        _ => Greeting::default(),
    };
    let name = args.get(2).cloned().unwrap_or("World".to_string());
    println!("{}, {}!", greeting, name);
}
RUST

echo "$ cargo build 2>&1"
cargo build 2>&1
echo

echo "$ cargo clippy -- -D warnings 2>&1"
if cargo clippy -- -D warnings 2>&1; then
    echo "(no warnings)"
else
    echo
    echo "--- Clippy found warnings! Recording trajectory with reward -1 ---"

    # Record failed trajectory
    mkdir -p "$SAGA_DIR/trajectories/rust-project-init"
    cat > "$SAGA_DIR/trajectories/rust-project-init/run_001.json" << 'TRAJ'
{
    "task_type": "rust-project-init",
    "state": {"project": "greeter"},
    "action": "cargo init + edition 2024 + wrote code with manual Default impl",
    "result": "clippy warning: derivable_impls",
    "reward": -1,
    "timestamp": "2026-03-21T10:00:00"
}
TRAJ
fi
echo

# Complete step 1 with clippy issue, advance to clippy-fix step
agentrail complete \
    --summary "Created project, edition 2024, but clippy has derivable_impls warning" \
    --next-slug fix-clippy \
    --next-prompt "Fix the clippy derivable_impls warning without using #[allow]" \
    --next-role production \
    --next-task-type clippy-fix

echo
echo "--- Step 2: Agent sees clippy-fix skill ---"
agentrail next
echo

echo "--- Following clippy-fix skill: apply the suggested fix ---"
agentrail begin

# Fix clippy warnings: derive Default + use all variants
cat > src/main.rs << 'RUST'
use std::fmt;

#[derive(Debug, Default)]
enum Greeting {
    #[default]
    Hello,
    Hi,
    Hey,
}

impl fmt::Display for Greeting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Greeting::Hello => write!(f, "Hello"),
            Greeting::Hi => write!(f, "Hi"),
            Greeting::Hey => write!(f, "Hey"),
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let greeting = match args.get(1).map(|s| s.as_str()) {
        Some("hi") => Greeting::Hi,
        Some("hey") => Greeting::Hey,
        _ => Greeting::default(),
    };
    let name = args.get(2).cloned().unwrap_or("World".to_string());
    println!("{}, {}!", greeting, name);
}
RUST

echo "$ cargo clippy -- -D warnings 2>&1"
cargo clippy -- -D warnings 2>&1
echo

echo "$ cargo fmt --check 2>&1"
cargo fmt --check 2>&1
echo "(clean)"
echo

echo "$ cargo run -- hey Rustacean"
cargo run -- hey Rustacean 2>/dev/null
echo

# Record success trajectory
mkdir -p "$SAGA_DIR/trajectories/clippy-fix"
cat > "$SAGA_DIR/trajectories/clippy-fix/run_001.json" << 'TRAJ'
{
    "task_type": "clippy-fix",
    "state": {"warning": "derivable_impls"},
    "action": "replaced manual Default impl with #[derive(Default)] and #[default]",
    "result": "zero clippy warnings",
    "reward": 1,
    "timestamp": "2026-03-21T10:05:00"
}
TRAJ

agentrail complete --summary "Fixed derivable_impls warning using #[derive(Default)] and #[default] attribute" --done
echo

echo "$ agentrail history"
agentrail history
echo

echo "$ agentrail status"
agentrail status
echo

echo "=== Demo complete! Project built, warnings fixed, saga finished ==="
rm -rf "$DEMO_DIR"
