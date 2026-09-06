//! Retroactive slug renaming.
//!
//! When two agents work the same repo on parallel branches, each writes
//! its own steps into `.agentrail/steps/`. The numeric prefix (`NNN-`)
//! is per-branch and will therefore repeat, so what keeps the merged
//! tree readable — and keeps the two branches from fighting over the
//! same directory — is the *slug*. A per-agent slug prefix
//! (`rtx5060-`, `rtx3060-`) makes every step directory unique across
//! branches, so a merge lands both lanes side by side instead of
//! conflicting.
//!
//! Renaming is deliberately **not** renumbering. `number`, `status`,
//! `commits`, and `completed_at` are all preserved, so a completed
//! step's git-history linkage survives the rename and `agentrail audit`
//! still matches it. That is why, unlike `insert`/`reorder`, this
//! module is happy to touch completed steps: work that has already
//! landed is exactly the history a retroactive rename exists to fix.

use crate::step;
use agentrail_core::error::{Error, Result};
use std::path::Path;

/// One planned directory rename: `NNN-old_slug` -> `NNN-new_slug`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepRename {
    pub number: u32,
    pub old_slug: String,
    pub new_slug: String,
}

impl StepRename {
    pub fn old_dir_name(&self) -> String {
        format!("{:03}-{}", self.number, self.old_slug)
    }

    pub fn new_dir_name(&self) -> String {
        format!("{:03}-{}", self.number, self.new_slug)
    }
}

/// Reject slugs that would escape `steps/`, shadow a temp directory, or
/// produce a name the rest of the store can't round-trip.
pub fn validate_slug(slug: &str) -> Result<()> {
    if slug.is_empty() {
        return Err(Error::Other("slug must not be empty".into()));
    }
    if slug != slug.trim() {
        return Err(Error::Other(format!(
            "slug {slug:?} has leading or trailing whitespace"
        )));
    }
    if slug.starts_with('.') {
        return Err(Error::Other(format!(
            "slug {slug:?} must not start with '.' (reserved for internal temp dirs)"
        )));
    }
    if let Some(bad) = slug
        .chars()
        .find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))
    {
        return Err(Error::Other(format!(
            "slug {slug:?} contains invalid character {bad:?} \
             (allowed: ASCII letters, digits, '-', '_', '.')"
        )));
    }
    Ok(())
}

/// Normalize a user-supplied prefix so `--prefix rtx5060` and
/// `--prefix rtx5060-` mean the same thing.
pub fn normalize_prefix(raw: &str) -> Result<String> {
    let trimmed = raw.trim().trim_end_matches(['-', '_']);
    if trimmed.is_empty() {
        return Err(Error::Other("prefix must not be empty".into()));
    }
    validate_slug(trimmed)?;
    Ok(trimmed.to_string())
}

/// Does `slug` already carry `prefix` as its leading segment?
fn has_prefix(slug: &str, prefix: &str) -> bool {
    slug.strip_prefix(prefix)
        .is_some_and(|rest| rest.starts_with('-'))
}

/// Plan a prefix rename across every step in the saga. Steps whose slug
/// already begins with `prefix-` are skipped, so re-running the command
/// is a no-op. Returned in step-number order.
pub fn plan_prefix(saga_dir: &Path, prefix: &str) -> Result<Vec<StepRename>> {
    let mut plan = Vec::new();
    for (_, cfg) in step::list_steps(saga_dir)? {
        if has_prefix(&cfg.slug, prefix) {
            continue;
        }
        let new_slug = format!("{prefix}-{}", cfg.slug);
        validate_slug(&new_slug)?;
        plan.push(StepRename {
            number: cfg.number,
            old_slug: cfg.slug,
            new_slug,
        });
    }
    Ok(plan)
}

/// Plan a rename of one step. `Ok(None)` means the slug already matches.
pub fn plan_one(saga_dir: &Path, number: u32, new_slug: &str) -> Result<Option<StepRename>> {
    validate_slug(new_slug)?;
    let dir = step::find_step_dir(saga_dir, number)
        .map_err(|_| Error::Other(format!("no step at position {number:03}")))?;
    let cfg = step::load_step(&dir)?;
    if cfg.slug == new_slug {
        return Ok(None);
    }
    Ok(Some(StepRename {
        number: cfg.number,
        old_slug: cfg.slug,
        new_slug: new_slug.to_string(),
    }))
}

/// Apply a rename plan. Validates the whole plan first — an unwritable
/// target, a duplicate, or an existing directory in the way aborts
/// before anything moves — then renames via a temp name per entry so an
/// in-plan swap can't clobber itself.
pub fn apply(saga_dir: &Path, renames: &[StepRename]) -> Result<()> {
    if renames.is_empty() {
        return Ok(());
    }
    let steps_dir = saga_dir.join("steps");

    let sources: Vec<String> = renames.iter().map(|r| r.old_dir_name()).collect();
    let mut targets: Vec<String> = Vec::with_capacity(renames.len());

    for r in renames {
        validate_slug(&r.new_slug)?;

        let source = steps_dir.join(r.old_dir_name());
        if !source.is_dir() {
            return Err(Error::Other(format!(
                "step directory {} not found",
                r.old_dir_name()
            )));
        }

        let target_name = r.new_dir_name();
        if targets.contains(&target_name) {
            return Err(Error::Other(format!(
                "rename plan maps two steps onto {target_name}"
            )));
        }
        // A target that is also a source is fine — the two-phase move
        // vacates it before anyone lands on it.
        if steps_dir.join(&target_name).exists() && !sources.contains(&target_name) {
            return Err(Error::Other(format!(
                "cannot rename {} -> {target_name}: that directory already exists",
                r.old_dir_name()
            )));
        }
        targets.push(target_name);
    }

    // Phase 1: park every source at a temp name.
    for (i, r) in renames.iter().enumerate() {
        let tmp = steps_dir.join(format!(".tmp.rename.{i}-{}", r.old_dir_name()));
        std::fs::rename(steps_dir.join(r.old_dir_name()), &tmp)?;
    }

    // Phase 2: land each at its target and rewrite the slug field.
    for (i, r) in renames.iter().enumerate() {
        let tmp = steps_dir.join(format!(".tmp.rename.{i}-{}", r.old_dir_name()));
        let final_dir = steps_dir.join(r.new_dir_name());
        std::fs::rename(&tmp, &final_dir)?;
        let mut cfg = step::load_step(&final_dir)?;
        cfg.slug = r.new_slug.clone();
        step::save_step(&final_dir, &cfg)?;
    }

    Ok(())
}

/// Prefix a saga name, or `None` if it already carries the prefix.
pub fn prefixed_saga_name(name: &str, prefix: &str) -> Option<String> {
    if has_prefix(name, prefix) {
        None
    } else {
        Some(format!("{prefix}-{name}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_prefix_requires_a_segment_boundary() {
        assert!(has_prefix("rtx5060-tune", "rtx5060"));
        assert!(!has_prefix("rtx50601-tune", "rtx5060"));
        assert!(!has_prefix("rtx5060", "rtx5060"));
        assert!(!has_prefix("tune", "rtx5060"));
    }
}
