use agentrail_core::error::Result;
use agentrail_store::size_guard::{self, Scope};
use std::path::Path;

/// Returns exit code: 0 = nothing over the block limit (warnings may have
/// been printed), 1 = at least one file must not be committed.
pub fn run(saga_path: &Path, staged: bool) -> Result<u8> {
    let scope = if staged {
        Scope::Staged
    } else {
        Scope::Committable
    };
    let found = size_guard::scan(saga_path, scope);
    let Some(report) = size_guard::format_report(&found) else {
        println!(
            "No oversize files under .agentrail/ or .agentrail-archive/ ({}).",
            if staged { "staged" } else { "committable" }
        );
        return Ok(0);
    };
    eprintln!("{report}");
    Ok(if found.iter().any(|f| f.blocked()) {
        1
    } else {
        0
    })
}
