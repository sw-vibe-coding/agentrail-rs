use agentrail_core::error::{Error, Result};
use std::path::{Path, PathBuf};

const DEFAULT_FILENAME: &str = "AGENTS.example.md";

const TEMPLATE: &str = include_str!("../../templates/AGENTS.example.md");

pub struct GenArgs<'a> {
    pub output: Option<&'a str>,
    pub force: bool,
}

pub fn run(saga_path: &Path, args: &GenArgs<'_>) -> Result<()> {
    let target: PathBuf = match args.output {
        Some(p) => PathBuf::from(p),
        None => saga_path.join(DEFAULT_FILENAME),
    };

    if target.exists() && !args.force {
        return Err(Error::Other(format!(
            "{} already exists. Pass --force to overwrite.",
            target.display()
        )));
    }

    if let Some(parent) = target.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(&target, TEMPLATE)?;
    println!("Wrote {} ({} bytes)", target.display(), TEMPLATE.len());
    println!();
    println!("This is a self-contained agent instructions template. Drop it into");
    println!("any project that uses agentrail — copy or rename to AGENTS.md,");
    println!("CLAUDE.md, or whatever your agent reads.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn writes_template_to_default_path() {
        let tmp = tempdir().unwrap();
        let args = GenArgs {
            output: None,
            force: false,
        };
        run(tmp.path(), &args).unwrap();
        let path = tmp.path().join(DEFAULT_FILENAME);
        assert!(path.is_file());
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.contains("agentrail"));
        assert!(body.contains("complete"));
    }

    #[test]
    fn refuses_overwrite_without_force() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join(DEFAULT_FILENAME);
        std::fs::write(&path, "existing").unwrap();

        let args = GenArgs {
            output: None,
            force: false,
        };
        let err = run(tmp.path(), &args).unwrap_err();
        assert!(err.to_string().contains("already exists"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "existing");
    }

    #[test]
    fn force_overwrites() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join(DEFAULT_FILENAME);
        std::fs::write(&path, "existing").unwrap();

        let args = GenArgs {
            output: None,
            force: true,
        };
        run(tmp.path(), &args).unwrap();
        let body = std::fs::read_to_string(&path).unwrap();
        assert_ne!(body, "existing");
        assert!(body.contains("agentrail"));
    }

    #[test]
    fn custom_output_path_is_honored() {
        let tmp = tempdir().unwrap();
        let custom = tmp.path().join("nested/elsewhere.md");
        let args = GenArgs {
            output: Some(custom.to_str().unwrap()),
            force: false,
        };
        run(tmp.path(), &args).unwrap();
        assert!(custom.is_file());
    }
}
