# Architecture: Two-Layer Design

AgentRail separates concerns into two independent layers with a clean
interface between them. This document describes the architecture, the
reasoning behind it, and how the layers interact.

## Layer 1: Generic Inference-Time Learning Engine

**Lives in**: `agentrail-rs` (this repo)

Layer 1 is the task-agnostic orchestration and memory layer. It manages
workflow state, dual-memory storage, trajectory retrieval, and prompt
injection. It does not know about any specific domain (TTS, video, Rust
builds, web deploys, etc.).

### Responsibilities

- **Workflow state machine**: sagas, steps, transitions, session snapshots
- **Dual memory (XSkill pattern)**: skills (strategic workflow docs per task
  category) and experiences (tactical per-run records with rewards)
- **ICRL injection**: retrieve relevant skills and experiences, format them
  into agent prompts via `agentrail next`
- **Distillation**: analyze batches of experiences, update/generate skill
  documents (`agentrail distill`)
- **Abstract execution**: route job specs to domain executors by `kind`,
  without importing domain-specific code
- **Abstract validation**: evaluate outputs against named validator contracts,
  without knowing what the validators check internally
- **Domain registry**: discover and load domain repos from configuration

### What Layer 1 knows about

- Sagas, steps, step roles, step transitions
- Skills (structured workflow documents per task_type)
- Experiences (state/action/result/reward + trigger conditions + failure modes)
- Executor trait: `fn execute(job: &JobSpec) -> Result<ExecutionResult>`
- Validator trait: `fn validate(contract: &str, context: &Value) -> Result<ValidationResult>`
- Task types as opaque strings (e.g., "tts", "ffmpeg-concat") -- meaningful
  only to domain repos

### What Layer 1 does NOT know about

- How TTS works, which API to call, what ffmpeg flags to use
- Domain-specific validation logic (duration checks, file format checks)
- Any concrete tool, service, or external system

## Layer 2: Domain-Specific Knowledge Repos

**Lives in**: separate per-domain GitHub repos (e.g., `agentrail-domain-media`)

Layer 2 contains all domain-specific knowledge: skill documents, curated
experience libraries, executor implementations, and validator implementations.
Each domain is an independent, versionable, shareable repo.

### Responsibilities

- **Skill documents**: authored and distilled procedure docs for each task type
  in the domain (e.g., "how to generate TTS audio", "how to concatenate videos")
- **Experience libraries**: curated successful trajectories and known failure
  modes, stored in the standard experience format
- **Executor implementations**: concrete code that runs deterministic steps
  (shell scripts, Rust binaries, API calls)
- **Validator implementations**: concrete checks (file exists, duration > 0,
  format is valid, output matches contract)
- **Task-type knowledge graphs** (optional): directed graphs of expected tool
  chains for structured reward signals

### Standard domain repo structure

```
agentrail-domain-{name}/
  domain.toml           # metadata: name, task types, executor kinds, validator kinds
  skills/
    {task_type}.toml     # skill document per task type
  experiences/
    {task_type}/
      run_NNN.json       # curated experience records
  executors/
    {kind}.sh            # or {kind}.rs, or {kind}.py -- executor implementations
  validators/
    {check_name}.sh      # or .rs, .py -- validator implementations
  graphs/
    {task_type}.toml     # optional: expected tool chain graph
```

### Example domains

- `agentrail-domain-media` -- TTS generation, ffmpeg operations, video
  compositing, audio normalization, duration probing
- `agentrail-domain-rust` -- cargo test, clippy, build patterns, release
  packaging
- `agentrail-domain-web` -- deploy, lighthouse audit, screenshot validation
- `agentrail-domain-content` -- blog post generation, markdown validation,
  image optimization

## Layer Interaction

```
Agent prompt
    |
    v
+---------------------------+
| Layer 1: agentrail-rs     |
|                           |
| agentrail next            |
|   1. Load step config     |
|   2. Check task_type      |
|   3. Retrieve skill doc   |  <-- from domain repo or local store
|   4. Retrieve top-N       |  <-- experiences matching task_type
|      successful experiences
|   5. Format into prompt   |
|   6. Output to agent      |
+---------------------------+
    |
    | (abstract interfaces)
    v
+---------------------------+
| Layer 2: domain repo      |
|                           |
| Provides:                 |
|   - Skill documents       |
|   - Experience records    |
|   - Executor binaries     |
|   - Validator checks      |
+---------------------------+
```

### Domain discovery

Layer 1 discovers domains through `.agentrail/domains.toml`:

```toml
[[domain]]
name = "media"
path = "/path/to/agentrail-domain-media"
# or: repo = "https://github.com/user/agentrail-domain-media"

[[domain]]
name = "rust"
path = "/path/to/agentrail-domain-rust"
```

When `agentrail next` encounters a step with `task_type = "tts"`, it:

1. Looks up which domain provides the "tts" task type (from domain.toml manifests)
2. Loads the skill document from that domain's `skills/tts.toml`
3. Loads experiences from both the domain's curated library and the local
   saga's experience store
4. Injects both into the prompt output

### Data flow on completion

When `agentrail complete` records a step:

1. The experience record is saved to the local saga store (`.agentrail/experiences/`)
2. Optionally, the experience can be contributed back to the domain repo
   (manual curation or `agentrail contribute` command)
3. `agentrail distill <task_type>` reads all experiences and updates the skill doc

## Research Foundations

This architecture is informed by specific research:

| Concept | Source | How it maps |
|---------|--------|-------------|
| Dual memory (skills + experiences) | XSkill (arXiv 2603.12056) | Layer 1 stores and retrieves both types |
| In-context RL from trajectories | Decision Transformer, Reflexion, Voyager (MLF-02a) | Layer 1 injects experiences as trajectory examples |
| Knowledge graphs as reward models | arXiv 2601.15160 (MLF-03a) | Layer 2 domain repos can define tool-chain graphs |
| Inference-time > weight updates | Sleepy Coder experiment | Layer 1 focuses entirely on context engineering |
| Accumulation/inference loop | XSkill two-phase design | `distill` = accumulation, `next` = inference |

## Why Not Weight-Based Learning Here?

The Sleepy Coder experiment (https://software-wrighter-lab.github.io/2026/02/12/sleepy-coder-when-fine-tuning-fails/)
showed that LoRA fine-tuning on a capable base model (Qwen2.5-Coder-1.5B)
could not improve beyond baseline (73.3%) and often caused catastrophic
forgetting (dropping to 60%). The Share algorithm prevented forgetting but
could not push past baseline.

The lesson: for models that already handle a task reasonably well, the
ceiling is inference-time context quality, not weight adjustment. Layers 1
and 2 maximize context quality. Weight-based approaches (LoRA, shared
subspaces, mHC, Engram) are deferred to separate future layers that can
consume the same experience data as training signal.
