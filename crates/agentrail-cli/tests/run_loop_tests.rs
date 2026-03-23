use agentrail_cli::commands::{init, run_loop};
use agentrail_core::{JobSpec, StepRole};
use agentrail_store::{saga, step};
use step::CreateStepParams;
use tempfile::tempdir;

/// Get the path to the mock domain in the repo
fn mock_domain_dir() -> std::path::PathBuf {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("demo/domain-mock")
}

/// Set up domains.toml pointing to the mock domain
fn register_mock_domain(saga_dir: &std::path::Path) {
    let domain_dir = mock_domain_dir();
    let config = format!(
        "[[domain]]\nname = \"mock\"\npath = \"{}\"\n",
        domain_dir.display()
    );
    std::fs::write(saga_dir.join("domains.toml"), config).unwrap();
}

#[test]
fn run_loop_pauses_at_production_step() {
    let tmp = tempdir().unwrap();
    init::run(tmp.path(), "test", "plan").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());

    // Create a production step (should pause)
    step::create_step(&CreateStepParams {
        saga_dir: &saga_dir,
        number: 1,
        slug: "agent-work",
        prompt: "Do something",
        description: "Agent step",
        role: StepRole::Production,
        context_files: &[],
        task_type: None,
        job_spec: None,
    })
    .unwrap();

    let mut config = saga::load_saga(tmp.path()).unwrap();
    config.current_step = 1;
    saga::save_saga(tmp.path(), &config).unwrap();

    let code = run_loop::run(tmp.path()).unwrap();
    assert_eq!(code, 0); // paused at agent step
}

#[test]
fn run_loop_executes_deterministic_step() {
    let tmp = tempdir().unwrap();
    init::run(tmp.path(), "test", "plan").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());
    register_mock_domain(&saga_dir);

    let output_path = tmp.path().join("output.txt");

    // Create a deterministic step with write-file job
    step::create_step(&CreateStepParams {
        saga_dir: &saga_dir,
        number: 1,
        slug: "write-it",
        prompt: "Write a file",
        description: "Write file step",
        role: StepRole::Deterministic,
        context_files: &[],
        task_type: Some("write-file"),
        job_spec: Some(JobSpec {
            kind: "write-file".to_string(),
            params: serde_json::json!({
                "output_path": output_path.to_string_lossy(),
                "content": "hello from agentrail"
            }),
        }),
    })
    .unwrap();

    let mut config = saga::load_saga(tmp.path()).unwrap();
    config.current_step = 1;
    saga::save_saga(tmp.path(), &config).unwrap();

    let code = run_loop::run(tmp.path()).unwrap();
    assert_eq!(code, 1); // completed, no more steps

    // Verify the file was actually created
    assert!(output_path.exists());
    let content = std::fs::read_to_string(&output_path).unwrap();
    assert!(content.contains("hello from agentrail"));

    // Verify step was marked completed
    let step_dir = step::find_step_dir(&saga_dir, 1).unwrap();
    let step_config = step::load_step(&step_dir).unwrap();
    assert_eq!(step_config.status, agentrail_core::StepStatus::Completed);

    // Verify trajectory was recorded
    let trajectories = agentrail_store::trajectory::load_all_trajectories(
        &saga_dir.join("trajectories/write-file"),
    )
    .unwrap();
    assert_eq!(trajectories.len(), 1);
    assert_eq!(trajectories[0].reward, 1);
}

#[test]
fn run_loop_chains_deterministic_then_pauses_at_production() {
    let tmp = tempdir().unwrap();
    init::run(tmp.path(), "test", "plan").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());
    register_mock_domain(&saga_dir);

    let output_path = tmp.path().join("step1.txt");

    // Step 1: deterministic (auto-execute)
    step::create_step(&CreateStepParams {
        saga_dir: &saga_dir,
        number: 1,
        slug: "auto-write",
        prompt: "Write a file",
        description: "Auto write",
        role: StepRole::Deterministic,
        context_files: &[],
        task_type: Some("write-file"),
        job_spec: Some(JobSpec {
            kind: "write-file".to_string(),
            params: serde_json::json!({
                "output_path": output_path.to_string_lossy(),
                "content": "auto-generated"
            }),
        }),
    })
    .unwrap();

    // Step 2: production (pause for agent)
    step::create_step(&CreateStepParams {
        saga_dir: &saga_dir,
        number: 2,
        slug: "review",
        prompt: "Review the output",
        description: "Agent reviews",
        role: StepRole::Production,
        context_files: &[],
        task_type: None,
        job_spec: None,
    })
    .unwrap();

    let mut config = saga::load_saga(tmp.path()).unwrap();
    config.current_step = 1;
    saga::save_saga(tmp.path(), &config).unwrap();

    let code = run_loop::run(tmp.path()).unwrap();
    assert_eq!(code, 0); // paused at step 2

    // Step 1 was auto-executed
    assert!(output_path.exists());
    let step1_dir = step::find_step_dir(&saga_dir, 1).unwrap();
    let step1 = step::load_step(&step1_dir).unwrap();
    assert_eq!(step1.status, agentrail_core::StepStatus::Completed);

    // Step 2 is still pending (paused for agent)
    let config = saga::load_saga(tmp.path()).unwrap();
    assert_eq!(config.current_step, 2);
}

#[test]
fn run_loop_echo_executor() {
    let tmp = tempdir().unwrap();
    init::run(tmp.path(), "test", "plan").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());
    register_mock_domain(&saga_dir);

    step::create_step(&CreateStepParams {
        saga_dir: &saga_dir,
        number: 1,
        slug: "echo-test",
        prompt: "Echo a message",
        description: "Echo",
        role: StepRole::Deterministic,
        context_files: &[],
        task_type: Some("echo"),
        job_spec: Some(JobSpec {
            kind: "echo".to_string(),
            params: serde_json::json!({"message": "hello world"}),
        }),
    })
    .unwrap();

    let mut config = saga::load_saga(tmp.path()).unwrap();
    config.current_step = 1;
    saga::save_saga(tmp.path(), &config).unwrap();

    let code = run_loop::run(tmp.path()).unwrap();
    assert_eq!(code, 1); // completed

    let step_dir = step::find_step_dir(&saga_dir, 1).unwrap();
    let step_config = step::load_step(&step_dir).unwrap();
    assert_eq!(step_config.status, agentrail_core::StepStatus::Completed);
}
