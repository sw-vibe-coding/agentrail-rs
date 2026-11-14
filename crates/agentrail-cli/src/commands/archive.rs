use agentrail_core::error::Result;
use std::path::Path;

pub fn run(saga_path: &Path, reason: Option<&str>) -> Result<()> {
    let (archive_dir, config) = agentrail_store::archive::archive_saga(saga_path, reason)?;

    let dir_name = archive_dir
        .strip_prefix(saga_path)
        .unwrap_or(&archive_dir)
        .display();

    println!(
        "Archived saga '{}' (status: {:?}) to {dir_name}/",
        config.name, config.status,
    );

    // An older saga may still carry session logs; archiving just moved
    // them into .agentrail-archive/, where they are as committable as ever.
    if let Some(msg) = agentrail_store::session::in_repo_warning(saga_path) {
        println!();
        println!("{msg}");
    }

    Ok(())
}
