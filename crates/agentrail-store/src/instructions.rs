//! Briefing / clearinghouse: shared agent instructions embedded into the
//! binary at compile time and stamped into target files (CLAUDE.md, AGENTS.md)
//! between markers.
//!
//! The `agent-instructions/` directory at the workspace root is the canonical
//! source of truth. Fragments are bundled at compile time via `include_str!`,
//! so the install path is: edit fragments → commit → `sw-install` (rebuild) →
//! `agentrail instructions apply` in each project.
//!
//! Local sections of CLAUDE.md / AGENTS.md outside the markered block are
//! preserved verbatim. Re-applying with the same embedded content is a no-op.

use agentrail_core::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::saga::saga_dir;

// ---------------------------------------------------------------------------
// Embedded source — compile-time bundle of fragments and profiles.
// ---------------------------------------------------------------------------

pub const EMBEDDED_FRAGMENTS: &[(&str, &str)] = &[
    (
        "global/baseline.md",
        include_str!("../../../agent-instructions/global/baseline.md"),
    ),
    (
        "global/session-protocol.md",
        include_str!("../../../agent-instructions/global/session-protocol.md"),
    ),
    (
        "global/agentrail-process.md",
        include_str!("../../../agent-instructions/global/agentrail-process.md"),
    ),
    (
        "global/push-discipline.md",
        include_str!("../../../agent-instructions/global/push-discipline.md"),
    ),
    (
        "global/recovery.md",
        include_str!("../../../agent-instructions/global/recovery.md"),
    ),
    (
        "global/git-hygiene.md",
        include_str!("../../../agent-instructions/global/git-hygiene.md"),
    ),
];

pub const EMBEDDED_PROFILES: &[(&str, &str)] = &[(
    "default",
    include_str!("../../../agent-instructions/profiles/default.toml"),
)];

fn lookup_fragment(path: &str) -> Option<&'static str> {
    EMBEDDED_FRAGMENTS
        .iter()
        .find_map(|(p, c)| if *p == path { Some(*c) } else { None })
}

fn lookup_profile(name: &str) -> Option<&'static str> {
    EMBEDDED_PROFILES
        .iter()
        .find_map(|(n, c)| if *n == name { Some(*c) } else { None })
}

// ---------------------------------------------------------------------------
// Schemas
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct EmbeddedProfile {
    #[allow(dead_code)]
    #[serde(default)]
    name: String,
    #[allow(dead_code)]
    #[serde(default)]
    description: String,
    include: Vec<String>,
}

/// Per-repo briefing config in `.agentrail/instruction-profile.toml`.
///
/// All fields are optional. Defaults: profile = "default", targets =
/// auto-detected from `CLAUDE.md` and `AGENTS.md` in the project root,
/// auto_apply = false.
#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct UserProfile {
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub targets: Vec<String>,
    /// When true, `agentrail next` (and other read paths) will rewrite the
    /// briefing block automatically when stale, instead of merely warning.
    /// Files are written but not committed — the agent must commit the
    /// refresh before `agentrail complete`.
    #[serde(default)]
    pub auto_apply: bool,
}

/// Lock file written to `.agentrail/instruction-lock.toml` after each apply.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct InstructionLock {
    pub profile: String,
    pub content_hash: String,
    pub updated_at: String,
    pub targets: Vec<String>,
}

// ---------------------------------------------------------------------------
// Markers
// ---------------------------------------------------------------------------

pub const MARKER_START_PREFIX: &str = "<!-- agentrail:global:start";
pub const MARKER_END: &str = "<!-- agentrail:global:end -->";

const PROFILE_FILENAME: &str = "instruction-profile.toml";
const LOCK_FILENAME: &str = "instruction-lock.toml";
const DEFAULT_TARGETS: &[&str] = &["CLAUDE.md", "AGENTS.md"];
const DEFAULT_PROFILE_NAME: &str = "default";

// ---------------------------------------------------------------------------
// Render & hash
// ---------------------------------------------------------------------------

/// Render the canonical body of a profile by concatenating its fragments.
/// Does NOT include the markers; use [`render_block`] for the full block.
pub fn render_body(profile_name: &str) -> Result<String> {
    let raw = lookup_profile(profile_name).ok_or_else(|| {
        Error::Other(format!(
            "Unknown agentrail briefing profile: '{profile_name}'. \
             Embedded profiles: {}",
            EMBEDDED_PROFILES
                .iter()
                .map(|(n, _)| *n)
                .collect::<Vec<_>>()
                .join(", ")
        ))
    })?;
    let profile: EmbeddedProfile = toml::from_str(raw)?;
    let mut out = String::new();
    for include in &profile.include {
        let frag = lookup_fragment(include).ok_or_else(|| {
            Error::Other(format!(
                "Briefing fragment '{include}' not embedded in this binary. \
                 Did you forget to add it to EMBEDDED_FRAGMENTS in \
                 agentrail-store/src/instructions.rs?"
            ))
        })?;
        out.push_str(frag.trim_end_matches('\n'));
        out.push_str("\n\n");
    }
    Ok(out.trim_end_matches('\n').to_string() + "\n")
}

/// Render the full markered block: start marker + body + end marker, with a
/// trailing newline. Stable for a given profile + body — re-renders are
/// byte-identical.
pub fn render_block(profile_name: &str) -> Result<String> {
    let body = render_body(profile_name)?;
    let hash = fnv1a_hex(&body);
    Ok(format!(
        "{MARKER_START_PREFIX} profile={profile_name} version={hash} -->\n\
         <!-- AUTOGENERATED BY `agentrail instructions apply` — DO NOT EDIT BY HAND -->\n\
         \n\
         {body}\n\
         {MARKER_END}\n"
    ))
}

/// Stable, deterministic hash. Not cryptographic — just enough to detect
/// content changes across runs without a hash crate dependency.
pub fn fnv1a_hex(s: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for &b in s.as_bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

// ---------------------------------------------------------------------------
// Marker block extraction & replacement
// ---------------------------------------------------------------------------

/// Byte range of the existing markered block (inclusive of both markers and
/// the trailing newline of the end marker), if present.
pub fn find_block(s: &str) -> Option<(usize, usize)> {
    let start_byte = s.find(MARKER_START_PREFIX)?;
    let after_start_line = match s[start_byte..].find('\n') {
        Some(i) => start_byte + i + 1,
        None => return None,
    };
    let end_rel = s[after_start_line..].find(MARKER_END)?;
    let end_byte = after_start_line + end_rel;
    let end_line_end = match s[end_byte..].find('\n') {
        Some(i) => end_byte + i + 1,
        None => s.len(),
    };
    Some((start_byte, end_line_end))
}

/// Extract the body between markers (without the markers themselves).
pub fn extract_body(s: &str) -> Option<String> {
    let (start_byte, end_byte) = find_block(s)?;
    let after_start_line = s[start_byte..].find('\n').map(|i| start_byte + i + 1)?;
    let end_marker_start = s[after_start_line..end_byte].rfind(MARKER_END)?;
    let body_end = after_start_line + end_marker_start;
    let body = &s[after_start_line..body_end];
    let trimmed = body
        .strip_prefix(
            "<!-- AUTOGENERATED BY `agentrail instructions apply` — DO NOT EDIT BY HAND -->\n",
        )
        .unwrap_or(body);
    let trimmed = trimmed.strip_prefix('\n').unwrap_or(trimmed);
    Some(trimmed.trim_end_matches('\n').to_string() + "\n")
}

/// Compute the byte position to insert a fresh briefing block when no
/// markers exist. After a leading H1 + blank line if present, else 0.
fn find_insert_pos(s: &str) -> usize {
    if (s.starts_with("# ") || s.starts_with("#\t"))
        && let Some(eol) = s.find('\n')
    {
        let mut pos = eol + 1;
        if s[pos..].starts_with('\n') {
            pos += 1;
        }
        return pos;
    }
    0
}

/// Replace an existing block, or insert one if none exists. Returns
/// `(new_text, had_block)`.
pub fn replace_or_insert_block(existing: &str, new_block: &str) -> (String, bool) {
    if let Some((start, end)) = find_block(existing) {
        let mut out = String::with_capacity(existing.len() + new_block.len());
        out.push_str(&existing[..start]);
        out.push_str(new_block);
        out.push_str(&existing[end..]);
        return (out, true);
    }

    let pos = find_insert_pos(existing);
    let mut out = String::with_capacity(existing.len() + new_block.len() + 2);
    out.push_str(&existing[..pos]);
    if pos > 0 && !existing[..pos].ends_with("\n\n") {
        if !existing[..pos].ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');
    }
    out.push_str(new_block);
    let tail = &existing[pos..];
    if !tail.is_empty() && !tail.starts_with('\n') {
        out.push('\n');
    }
    out.push_str(tail);
    (out, false)
}

// ---------------------------------------------------------------------------
// Profile / target resolution
// ---------------------------------------------------------------------------

pub fn load_user_profile(saga_path: &Path) -> Result<UserProfile> {
    let p = saga_dir(saga_path).join(PROFILE_FILENAME);
    if !p.is_file() {
        return Ok(UserProfile::default());
    }
    let raw = std::fs::read_to_string(&p)?;
    Ok(toml::from_str(&raw)?)
}

pub fn save_user_profile(saga_path: &Path, profile: &UserProfile) -> Result<()> {
    let dir = saga_dir(saga_path);
    if !dir.is_dir() {
        std::fs::create_dir_all(&dir)?;
    }
    let p = dir.join(PROFILE_FILENAME);
    std::fs::write(&p, toml::to_string_pretty(profile)?)?;
    Ok(())
}

pub fn load_lock(saga_path: &Path) -> Result<Option<InstructionLock>> {
    let p = saga_dir(saga_path).join(LOCK_FILENAME);
    if !p.is_file() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&p)?;
    Ok(Some(toml::from_str(&raw)?))
}

pub fn save_lock(saga_path: &Path, lock: &InstructionLock) -> Result<()> {
    let dir = saga_dir(saga_path);
    if !dir.is_dir() {
        std::fs::create_dir_all(&dir)?;
    }
    let p = dir.join(LOCK_FILENAME);
    std::fs::write(&p, toml::to_string_pretty(lock)?)?;
    Ok(())
}

/// Resolve the profile name and target file list for a project, honoring
/// `.agentrail/instruction-profile.toml` overrides. Targets are returned as
/// absolute paths.
pub fn resolve_targets(saga_path: &Path) -> Result<(String, Vec<PathBuf>)> {
    let user = load_user_profile(saga_path)?;
    let profile = user
        .profile
        .clone()
        .unwrap_or_else(|| DEFAULT_PROFILE_NAME.to_string());

    let targets: Vec<PathBuf> = if user.targets.is_empty() {
        DEFAULT_TARGETS
            .iter()
            .map(|n| saga_path.join(n))
            .filter(|p| p.is_file())
            .collect()
    } else {
        user.targets.iter().map(|n| saga_path.join(n)).collect()
    };

    Ok((profile, targets))
}

// ---------------------------------------------------------------------------
// Apply
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ApplyOutcome {
    pub target: PathBuf,
    pub changed: bool,
    pub had_block: bool,
    pub created_file: bool,
}

/// Apply the rendered block to a single target file. Creates the file if it
/// does not exist (with the briefing as the only content).
pub fn apply_to_file(target: &Path, profile_name: &str) -> Result<ApplyOutcome> {
    let block = render_block(profile_name)?;
    let (existing, created_file) = match std::fs::read_to_string(target) {
        Ok(s) => (s, false),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (String::new(), true),
        Err(e) => return Err(e.into()),
    };

    let (new_text, had_block) = if created_file {
        (block.clone(), false)
    } else {
        replace_or_insert_block(&existing, &block)
    };

    let changed = created_file || new_text != existing;
    if changed {
        if let Some(parent) = target.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(target, &new_text)?;
    }

    Ok(ApplyOutcome {
        target: target.to_path_buf(),
        changed,
        had_block,
        created_file,
    })
}

/// Apply to every resolved target and write the lock file. Returns one
/// outcome per target.
pub fn apply(saga_path: &Path) -> Result<(String, Vec<ApplyOutcome>)> {
    let (profile, targets) = resolve_targets(saga_path)?;
    if targets.is_empty() {
        return Err(Error::Other(
            "No briefing targets found. Create CLAUDE.md or AGENTS.md, or set \
             `targets = [...]` in .agentrail/instruction-profile.toml."
                .to_string(),
        ));
    }

    let mut outcomes = Vec::with_capacity(targets.len());
    for t in &targets {
        outcomes.push(apply_to_file(t, &profile)?);
    }

    let body = render_body(&profile)?;
    let lock = InstructionLock {
        profile: profile.clone(),
        content_hash: fnv1a_hex(&body),
        updated_at: agentrail_core::timestamp_iso(),
        targets: targets
            .iter()
            .map(|p| {
                p.strip_prefix(saga_path)
                    .unwrap_or(p)
                    .to_string_lossy()
                    .to_string()
            })
            .collect(),
    };
    save_lock(saga_path, &lock)?;
    Ok((profile, outcomes))
}

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetStatus {
    Missing,
    NoBlock,
    UpToDate,
    Stale,
}

#[derive(Debug, Clone)]
pub struct TargetReport {
    pub target: PathBuf,
    pub status: TargetStatus,
    pub current_hash: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StatusReport {
    pub profile: String,
    pub embedded_hash: String,
    pub lock: Option<InstructionLock>,
    pub targets: Vec<TargetReport>,
}

pub fn status(saga_path: &Path) -> Result<StatusReport> {
    let (profile, targets) = resolve_targets(saga_path)?;
    let body = render_body(&profile)?;
    let embedded_hash = fnv1a_hex(&body);
    let lock = load_lock(saga_path)?;

    let mut reports = Vec::with_capacity(targets.len());
    for t in &targets {
        let report = match std::fs::read_to_string(t) {
            Ok(content) => match extract_body(&content) {
                Some(body) => {
                    let h = fnv1a_hex(&body);
                    let status = if h == embedded_hash {
                        TargetStatus::UpToDate
                    } else {
                        TargetStatus::Stale
                    };
                    TargetReport {
                        target: t.clone(),
                        status,
                        current_hash: Some(h),
                    }
                }
                None => TargetReport {
                    target: t.clone(),
                    status: TargetStatus::NoBlock,
                    current_hash: None,
                },
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => TargetReport {
                target: t.clone(),
                status: TargetStatus::Missing,
                current_hash: None,
            },
            Err(e) => return Err(e.into()),
        };
        reports.push(report);
    }

    Ok(StatusReport {
        profile,
        embedded_hash,
        lock,
        targets: reports,
    })
}

// ---------------------------------------------------------------------------
// Freshness check (used by `agentrail next` / `agentrail status`)
// ---------------------------------------------------------------------------

/// Has this repo opted into the briefing system? Returns true when the user
/// has committed to it via either an `instruction-profile.toml` or a
/// previously-written `instruction-lock.toml`. Without one of those, the
/// freshness check stays silent so we don't nag repos that haven't adopted
/// the briefing.
pub fn opted_in(saga_path: &Path) -> bool {
    let dir = saga_dir(saga_path);
    dir.join(PROFILE_FILENAME).is_file() || dir.join(LOCK_FILENAME).is_file()
}

/// One-line summary of why a repo's briefing is stale, for printing as a
/// banner in `agentrail next`. Returns `None` if the repo is up to date or
/// has not opted into the briefing.
pub fn freshness_warning(saga_path: &Path) -> Result<Option<String>> {
    if !opted_in(saga_path) {
        return Ok(None);
    }
    let report = status(saga_path)?;
    let stale_targets: Vec<_> = report
        .targets
        .iter()
        .filter(|t| t.status != TargetStatus::UpToDate)
        .collect();

    let lock_mismatch = report
        .lock
        .as_ref()
        .is_some_and(|l| l.content_hash != report.embedded_hash);

    if stale_targets.is_empty() && !lock_mismatch {
        return Ok(None);
    }

    let mut msg = String::new();
    msg.push_str("⚠ Briefing stale: embedded=");
    msg.push_str(&report.embedded_hash);
    if let Some(lock) = &report.lock {
        msg.push_str(", lock=");
        msg.push_str(&lock.content_hash);
    } else {
        msg.push_str(", lock=(none)");
    }
    msg.push('\n');
    for t in &stale_targets {
        let label = match t.status {
            TargetStatus::Stale => "stale",
            TargetStatus::NoBlock => "no briefing block",
            TargetStatus::Missing => "missing target",
            TargetStatus::UpToDate => continue,
        };
        msg.push_str(&format!("    {}: {}\n", t.target.display(), label));
    }
    msg.push_str(
        "  Run `agentrail instructions apply`, commit, then re-read CLAUDE.md \
         before proceeding.",
    );
    Ok(Some(msg))
}

/// If the briefing is stale and `auto_apply = true` is set in the user
/// profile, refresh the block in-place and return a "we just rewrote your
/// CLAUDE.md, re-read it" notice. Otherwise return the regular warning.
///
/// Used by `agentrail next`. The auto-apply path writes files but does NOT
/// commit them — the agent (or the user) must include the refreshed files
/// in the next commit before `agentrail complete`.
pub fn freshness_check_with_auto_apply(saga_path: &Path) -> Result<Option<String>> {
    let warn = freshness_warning(saga_path)?;
    let Some(warn) = warn else {
        return Ok(None);
    };
    let user = load_user_profile(saga_path).unwrap_or_default();
    if !user.auto_apply {
        return Ok(Some(warn));
    }

    // Auto-apply path: refresh the block, then return a refreshed-notice.
    let (profile, outcomes) = apply(saga_path)?;
    let refreshed: Vec<String> = outcomes
        .iter()
        .filter(|o| o.changed)
        .map(|o| o.target.display().to_string())
        .collect();
    let mut notice = format!(
        "✱ Briefing was auto-refreshed (profile '{profile}'). Re-read CLAUDE.md \
         before proceeding.\n"
    );
    for r in &refreshed {
        notice.push_str(&format!("    refreshed: {r}\n"));
    }
    notice.push_str(
        "  The refresh modified tracked files. Stage them with the rest of \
         your commit before `agentrail complete`.",
    );
    Ok(Some(notice))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn fnv1a_is_stable_and_distinct() {
        assert_eq!(fnv1a_hex("hello"), fnv1a_hex("hello"));
        assert_ne!(fnv1a_hex("hello"), fnv1a_hex("hello "));
        // Length is exactly 16 hex chars
        assert_eq!(fnv1a_hex("anything").len(), 16);
    }

    #[test]
    fn embedded_default_profile_renders() {
        let body = render_body("default").unwrap();
        assert!(body.contains("Agentrail metadata discipline"));
        assert!(body.contains("Git hygiene"));
        assert!(body.contains("Baseline expectations"));
        // Body must not contain the markers themselves
        assert!(!body.contains(MARKER_START_PREFIX));
        assert!(!body.contains(MARKER_END));
    }

    #[test]
    fn unknown_profile_errors() {
        let err = render_body("does-not-exist").unwrap_err();
        assert!(
            err.to_string()
                .contains("Unknown agentrail briefing profile")
        );
    }

    #[test]
    fn render_block_round_trips_via_extract_body() {
        let block = render_block("default").unwrap();
        let extracted = extract_body(&block).expect("block parses");
        let body = render_body("default").unwrap();
        assert_eq!(extracted, body);
    }

    #[test]
    fn replace_or_insert_into_empty_string() {
        let block = "<!-- agentrail:global:start profile=x version=y -->\nhi\n<!-- agentrail:global:end -->\n";
        let (out, had) = replace_or_insert_block("", block);
        assert!(!had);
        assert_eq!(out, block);
    }

    #[test]
    fn replace_or_insert_after_h1() {
        let existing = "# Project\n\nLocal notes here.\n";
        let block = "<!-- agentrail:global:start profile=x version=y -->\nbody\n<!-- agentrail:global:end -->\n";
        let (out, had) = replace_or_insert_block(existing, block);
        assert!(!had);
        assert!(out.starts_with("# Project\n\n<!-- agentrail:global:start"));
        assert!(out.contains("Local notes here."));
    }

    #[test]
    fn replace_existing_block_preserves_local_sections() {
        let existing = "# Project\n\n<!-- agentrail:global:start profile=x version=old -->\nold body\n<!-- agentrail:global:end -->\n\n## Local rules\n\nKeep this.\n";
        let new_block = "<!-- agentrail:global:start profile=x version=new -->\nnew body\n<!-- agentrail:global:end -->\n";
        let (out, had) = replace_or_insert_block(existing, new_block);
        assert!(had);
        assert!(out.contains("new body"));
        assert!(!out.contains("old body"));
        assert!(out.contains("## Local rules"));
        assert!(out.contains("Keep this."));
    }

    #[test]
    fn apply_to_missing_file_creates_it() {
        let tmp = tempdir().unwrap();
        let target = tmp.path().join("AGENTS.md");
        let outcome = apply_to_file(&target, "default").unwrap();
        assert!(outcome.created_file);
        assert!(outcome.changed);
        assert!(target.is_file());
        let body = std::fs::read_to_string(&target).unwrap();
        assert!(body.contains("Agentrail metadata discipline"));
    }

    #[test]
    fn apply_is_idempotent() {
        let tmp = tempdir().unwrap();
        let target = tmp.path().join("CLAUDE.md");
        std::fs::write(&target, "# Project\n\nLocal notes.\n").unwrap();

        let first = apply_to_file(&target, "default").unwrap();
        assert!(first.changed);

        let second = apply_to_file(&target, "default").unwrap();
        assert!(!second.changed, "second apply must be a no-op");
    }

    #[test]
    fn apply_full_writes_lock_and_picks_up_existing_targets() {
        let tmp = tempdir().unwrap();
        let saga_dir = tmp.path().join(".agentrail");
        std::fs::create_dir_all(&saga_dir).unwrap();
        std::fs::write(tmp.path().join("CLAUDE.md"), "# CLAUDE.md\n\nlocal\n").unwrap();

        let (profile, outcomes) = apply(tmp.path()).unwrap();
        assert_eq!(profile, "default");
        assert_eq!(outcomes.len(), 1);
        assert!(outcomes[0].changed);

        let lock = load_lock(tmp.path()).unwrap().unwrap();
        assert_eq!(lock.profile, "default");
        assert_eq!(lock.targets, vec!["CLAUDE.md".to_string()]);
        assert_eq!(lock.content_hash.len(), 16);
    }

    #[test]
    fn apply_errors_when_no_targets_resolvable() {
        let tmp = tempdir().unwrap();
        // No CLAUDE.md, no AGENTS.md, no profile config
        let err = apply(tmp.path()).unwrap_err();
        assert!(err.to_string().contains("No briefing targets"));
    }

    #[test]
    fn status_reports_up_to_date_after_apply() {
        let tmp = tempdir().unwrap();
        std::fs::write(tmp.path().join("AGENTS.md"), "# Project\n").unwrap();
        apply(tmp.path()).unwrap();

        let report = status(tmp.path()).unwrap();
        assert_eq!(report.profile, "default");
        assert_eq!(report.targets.len(), 1);
        assert_eq!(report.targets[0].status, TargetStatus::UpToDate);
        assert!(report.lock.is_some());
    }

    #[test]
    fn status_reports_stale_when_block_diverges() {
        let tmp = tempdir().unwrap();
        let target = tmp.path().join("AGENTS.md");
        std::fs::write(
            &target,
            "# Project\n\n<!-- agentrail:global:start profile=default version=stale -->\nstale body\n<!-- agentrail:global:end -->\n",
        )
        .unwrap();

        let report = status(tmp.path()).unwrap();
        assert_eq!(report.targets.len(), 1);
        assert_eq!(report.targets[0].status, TargetStatus::Stale);
    }

    #[test]
    fn status_reports_no_block_when_markers_missing() {
        let tmp = tempdir().unwrap();
        std::fs::write(
            tmp.path().join("AGENTS.md"),
            "# Project\n\nNo briefing here.\n",
        )
        .unwrap();

        let report = status(tmp.path()).unwrap();
        assert_eq!(report.targets[0].status, TargetStatus::NoBlock);
    }

    #[test]
    fn user_profile_overrides_targets() {
        let tmp = tempdir().unwrap();
        let saga = tmp.path().join(".agentrail");
        std::fs::create_dir_all(&saga).unwrap();
        std::fs::write(
            saga.join("instruction-profile.toml"),
            "profile = \"default\"\ntargets = [\"docs/RULES.md\"]\n",
        )
        .unwrap();
        std::fs::create_dir_all(tmp.path().join("docs")).unwrap();

        apply(tmp.path()).unwrap();
        let body = std::fs::read_to_string(tmp.path().join("docs/RULES.md")).unwrap();
        assert!(body.contains("agentrail:global:start"));

        let lock = load_lock(tmp.path()).unwrap().unwrap();
        assert_eq!(lock.targets, vec!["docs/RULES.md".to_string()]);
    }

    #[test]
    fn freshness_silent_when_not_opted_in() {
        let tmp = tempdir().unwrap();
        // Bare repo: no profile, no lock, no targets — never adopted briefing.
        assert!(!opted_in(tmp.path()));
        assert!(freshness_warning(tmp.path()).unwrap().is_none());
    }

    #[test]
    fn freshness_silent_when_up_to_date() {
        let tmp = tempdir().unwrap();
        std::fs::write(tmp.path().join("CLAUDE.md"), "# P\n").unwrap();
        apply(tmp.path()).unwrap();
        assert!(opted_in(tmp.path()));
        assert!(freshness_warning(tmp.path()).unwrap().is_none());
    }

    #[test]
    fn freshness_warns_when_block_drifts() {
        let tmp = tempdir().unwrap();
        std::fs::write(tmp.path().join("CLAUDE.md"), "# P\n").unwrap();
        apply(tmp.path()).unwrap();

        // Tamper inside the block.
        let path = tmp.path().join("CLAUDE.md");
        let body = std::fs::read_to_string(&path).unwrap();
        let drifted = body.replace("Git hygiene", "Git hygiene (TAMPERED)");
        std::fs::write(&path, drifted).unwrap();

        let warn = freshness_warning(tmp.path()).unwrap().unwrap();
        assert!(warn.contains("Briefing stale"));
        assert!(warn.contains("CLAUDE.md"));
        assert!(warn.contains("agentrail instructions apply"));
    }

    #[test]
    fn freshness_warns_when_lock_pre_dates_embedded() {
        let tmp = tempdir().unwrap();
        let saga = tmp.path().join(".agentrail");
        std::fs::create_dir_all(&saga).unwrap();
        // Lock says it was applied with hash X, but X is not the embedded hash.
        let stale_lock = "profile = \"default\"\ncontent_hash = \"deadbeefdeadbeef\"\nupdated_at = \"2026-01-01T00:00:00\"\ntargets = [\"CLAUDE.md\"]\n";
        std::fs::write(saga.join("instruction-lock.toml"), stale_lock).unwrap();
        std::fs::write(tmp.path().join("CLAUDE.md"), "# P\n").unwrap();

        let warn = freshness_warning(tmp.path()).unwrap().unwrap();
        assert!(warn.contains("Briefing stale"));
        assert!(warn.contains("lock=deadbeefdeadbeef"));
    }

    #[test]
    fn auto_apply_refreshes_and_returns_notice() {
        let tmp = tempdir().unwrap();
        let saga = tmp.path().join(".agentrail");
        std::fs::create_dir_all(&saga).unwrap();
        std::fs::write(
            saga.join("instruction-profile.toml"),
            "profile = \"default\"\nauto_apply = true\n",
        )
        .unwrap();
        std::fs::write(tmp.path().join("CLAUDE.md"), "# P\n").unwrap();

        let msg = freshness_check_with_auto_apply(tmp.path())
            .unwrap()
            .unwrap();
        assert!(msg.contains("auto-refreshed"));
        assert!(msg.contains("Re-read CLAUDE.md"));

        // Block was actually written.
        let body = std::fs::read_to_string(tmp.path().join("CLAUDE.md")).unwrap();
        assert!(body.contains("agentrail:global:start"));

        // Now up to date — no further warning.
        assert!(freshness_warning(tmp.path()).unwrap().is_none());
    }

    #[test]
    fn auto_apply_off_just_warns() {
        let tmp = tempdir().unwrap();
        let saga = tmp.path().join(".agentrail");
        std::fs::create_dir_all(&saga).unwrap();
        std::fs::write(
            saga.join("instruction-profile.toml"),
            "profile = \"default\"\n",
        )
        .unwrap();
        std::fs::write(tmp.path().join("CLAUDE.md"), "# P\n").unwrap();

        let msg = freshness_check_with_auto_apply(tmp.path())
            .unwrap()
            .unwrap();
        assert!(msg.contains("Briefing stale"));
        assert!(!msg.contains("auto-refreshed"));

        // CLAUDE.md was NOT modified.
        let body = std::fs::read_to_string(tmp.path().join("CLAUDE.md")).unwrap();
        assert!(!body.contains("agentrail:global:start"));
    }
}
