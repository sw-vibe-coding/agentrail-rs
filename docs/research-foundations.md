# Research Foundations

This document maps the research papers and experiments that inform
agentrail-rs design decisions. It explains what each source contributes
and where those ideas appear in the architecture.

## In-Context Reinforcement Learning (ICRL)

**Source**: MLF-02a episode; Decision Transformer (arXiv 2106.01345),
Transformers Learn TD Methods (arXiv 2405.13861), OmniRL (arXiv 2502.02869),
Reflexion (arXiv 2303.11366), Voyager (arXiv 2305.16291)

**Core idea**: Transformers can approximate RL policies from (state, action,
reward) sequences embedded in the context window. No weight updates needed.
The model learns to imitate rewarded trajectories during the forward pass.

**Where it appears in agentrail-rs**:
- Experience records store (state, action, result, reward) tuples
- `agentrail next` retrieves successful experiences and injects them into
  the agent's prompt
- The agent sees proven execution paths and biases toward them
- This is the primary mechanism for inference-time "learning"

**Key insight for agents**: AI coding agents lose procedural knowledge when
context is truncated between sessions. By embedding successful execution
traces, agents reuse proven approaches instead of improvising. Success rate
improves from ~75% to near-deterministic on repeated task types.

## XSkill: Dual Memory Architecture

**Source**: XSkill (arXiv 2603.12056); blog post at
https://software-wrighter-lab.github.io/2026/03/17/ai-tools-xskill-memory-layer-multimodal-agents/

**Core idea**: Agents need two complementary memory types:
1. **Skills**: strategic workflow documents for task categories
2. **Experiences**: tactical situation-specific lessons with triggers and
   failure documentation

Ablation studies confirm neither alone is sufficient. Skills provide
strategic structure; experiences provide tactical grounding.

**Where it appears in agentrail-rs**:
- Layer 1 stores and retrieves both skills and experiences
- Skills are structured TOML documents with procedures, success patterns,
  and common failure modes (see docs/dual-memory.md)
- Experiences are rich JSON records extending the original Trajectory type
- `agentrail distill` implements the accumulation phase (analyze experience
  batches, update skill docs)
- `agentrail next` implements the inference phase (retrieve and inject)

**Key results from XSkill**: syntax errors decreased from 20.3% to 11.4%,
tool identification errors fell from 2.85% to 0.32%, code interpreter
adoption increased from 66.6% to 77.0%.

## Knowledge Graphs as Implicit Reward Models

**Source**: MLF-03a episode; Knowledge Graphs are Implicit Reward Models
(arXiv 2601.15160), Alternative Trajectory for Generative AI
(arXiv 2603.14147), Bottom-up Domain-Specific Superintelligence
(arXiv 2507.13966), GraphMERT (arXiv 2510.09580)

**Core idea**: Knowledge graphs serve dual purpose:
1. Curriculum generator: extract reasoning tasks from graph structure
2. Implicit reward model: graph paths provide verifiable reward signals

Every reasoning path in the graph becomes a verifiable signal. Unlike
black-box reward models, transparency is built in.

**Where it appears in agentrail-rs**:
- Domain repos can define expected tool chains as directed graphs
  (`graphs/{task_type}.toml`)
- Layer 1 compares actual execution paths against expected graph edges
- Structured rewards: not just "did it work?" but "was the reasoning
  path valid?"
- This replaces the bare `i8` reward with graph-path-based verification

**Applicability note**: KG rewards are most useful for deterministic task
types with well-defined tool chains (TTS, ffmpeg, build pipelines). For
open-ended production steps, simple success/failure rewards are sufficient.

## Sleepy Coder: Why Not Weight-Based Learning?

**Source**: Sleepy Coder experiment at
https://software-wrighter-lab.github.io/2026/02/12/sleepy-coder-when-fine-tuning-fails/

**Core finding**: LoRA fine-tuning on a capable base model (Qwen2.5-Coder-1.5B)
could not improve beyond baseline (73.3%) and naive LoRA caused catastrophic
forgetting (dropping to 60%). The Share algorithm (shared LoRA subspaces,
arXiv 2602.06043) prevented forgetting but could not push past baseline.

**Key lesson**: For models that already handle a task reasonably well, the
performance ceiling is inference-time context quality, not weight adjustment.

**Where it appears in agentrail-rs**:
- Layers 1 and 2 focus entirely on inference-time context engineering
- No weight modification in the current architecture
- Experience and skill data could later be used as training signal for a
  separate Layer 3 (overnight LoRA with proper task routing), but that is
  out of scope for now
- The architecture does not preclude weight-based learning; it just does
  not depend on it

## Neural Collapse (Background)

**Source**: MLF-01 episode; Neural Collapse papers (arXiv 2505.15239,
arXiv 2501.19104, arXiv 2505.24254)

**Core idea**: During late training, class representations converge to a
symmetric simplex structure. This explains generalization in
overparameterized networks and has implications for continual learning.

**Relevance to agentrail-rs**: Indirect. Neural collapse theory supports
the understanding that small model weight adjustments cascade unpredictably
(reinforcing the Sleepy Coder finding). Progressive neural collapse has
implications for the future Layer 3 work on continual learning without
catastrophic forgetting.

## How the Research Layers Map to Future Work

| Layer | Focus | Research basis | Status |
|-------|-------|---------------|--------|
| 1 | Inference-time context engineering | ICRL, XSkill | Active (this repo) |
| 2 | Domain-specific knowledge | XSkill (domain skills), KG rewards | Next (domain repos) |
| 3 | Overnight weight adjustment | Sleepy Coder, Share LoRA, UWSH | Future |
| 4 | Model architecture improvements | mHC, Engram | Future |

Layers 1 and 2 are the focus of current development. Layers 3 and 4 can
consume the same experience data as training signal when the time comes.
