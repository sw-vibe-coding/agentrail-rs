//! Domain registry: discover and load domain repos.

use agentrail_core::error::{Error, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// A registered domain from .agentrail/domains.toml.
#[derive(Debug, Clone, Deserialize)]
pub struct DomainEntry {
    pub name: String,
    pub path: String,
}

/// A domain manifest from domain.toml inside a domain repo.
#[derive(Debug, Clone, Deserialize)]
pub struct DomainManifest {
    pub domain: DomainInfo,
    #[serde(default)]
    pub task_types: Vec<TaskTypeEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DomainInfo {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TaskTypeEntry {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub executor: String,
    #[serde(default)]
    pub validators: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct DomainsConfig {
    #[serde(default)]
    domain: Vec<DomainEntry>,
}

/// Load the domain registry from .agentrail/domains.toml.
pub fn load_domains(saga_dir: &Path) -> Result<Vec<DomainEntry>> {
    let path = saga_dir.join("domains.toml");
    if !path.is_file() {
        return Ok(vec![]);
    }
    let content = std::fs::read_to_string(&path)?;
    let config: DomainsConfig = toml::from_str(&content)?;
    Ok(config.domain)
}

/// Load a domain manifest from a domain repo directory.
pub fn load_manifest(domain_dir: &Path) -> Result<DomainManifest> {
    let path = domain_dir.join("domain.toml");
    if !path.is_file() {
        return Err(Error::Other(format!(
            "No domain.toml found in {}",
            domain_dir.display()
        )));
    }
    let content = std::fs::read_to_string(&path)?;
    let manifest: DomainManifest = toml::from_str(&content)?;
    Ok(manifest)
}

/// Find which domain provides a given task type.
/// Returns (domain_dir, task_type_entry).
pub fn find_domain_for_task(
    saga_dir: &Path,
    task_type: &str,
) -> Result<Option<(PathBuf, TaskTypeEntry)>> {
    let domains = load_domains(saga_dir)?;
    for entry in &domains {
        let domain_dir = PathBuf::from(&entry.path);
        if let Ok(manifest) = load_manifest(&domain_dir) {
            for tt in &manifest.task_types {
                if tt.name == task_type {
                    return Ok(Some((domain_dir, tt.clone())));
                }
            }
        }
    }
    Ok(None)
}
