//! `agentrail view` — generate a self-contained HTML report of saga state
//! and open it in the default browser.
//!
//! Single-file output (no external assets, no fonts, no images). Tab
//! switching is ~20 lines of vanilla JS; collapse uses native `<details>`.
//! System-aware dark/light theme via `prefers-color-scheme`, dark-leaning
//! defaults.

use agentrail_core::error::Result;
use agentrail_core::{SagaConfig, SagaStatus, StepConfig, StepStatus, Trajectory};
use agentrail_store::{archive, instructions, saga, step, trajectory};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct ViewArgs<'a> {
    /// Output HTML path (default: `.agentrail/view.html`).
    pub output: Option<&'a str>,
    /// Don't try to open the browser; print the path/URL instead.
    pub no_open: bool,
}

pub fn run(saga_path: &Path, args: &ViewArgs<'_>) -> Result<()> {
    let model = build_view_model(saga_path)?;
    let html = render_html(&model);

    let target = match args.output {
        Some(p) => PathBuf::from(p),
        None => default_output_path(saga_path)?,
    };
    if let Some(parent) = target.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&target, &html)?;

    let abs = std::fs::canonicalize(&target).unwrap_or_else(|_| target.clone());
    println!("Wrote {}", abs.display());

    if args.no_open {
        println!("file://{}", abs.display());
        return Ok(());
    }
    if let Err(e) = open_in_browser(&abs) {
        println!("Could not open browser ({e}); open this URL manually:");
        println!("file://{}", abs.display());
    }
    Ok(())
}

fn default_output_path(saga_path: &Path) -> Result<PathBuf> {
    let dir = saga_path.join(".agentrail");
    if !dir.is_dir() {
        // Saga not yet inited — write next to the project root instead.
        return Ok(saga_path.join("agentrail-view.html"));
    }
    Ok(dir.join("view.html"))
}

fn open_in_browser(path: &Path) -> std::io::Result<()> {
    let cmd = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "start"
    } else {
        "xdg-open"
    };
    let status = Command::new(cmd).arg(path).status()?;
    if !status.success() {
        return Err(std::io::Error::other(format!(
            "{cmd} exited with {status}"
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// View model
// ---------------------------------------------------------------------------

struct ViewModel {
    saga_path: PathBuf,
    current: Option<CurrentSagaModel>,
    archives: Vec<ArchiveSagaModel>,
    briefing: Option<String>,
    github_repo: Option<String>,
    summary: SummaryStrip,
}

struct CurrentSagaModel {
    config: SagaConfig,
    plan: String,
    steps: Vec<StepModel>,
    pct_complete: u32,
}

struct ArchiveSagaModel {
    config: SagaConfig,
    suffix: String,
    reason: Option<String>,
    steps: Vec<StepModel>,
    pct_complete: u32,
}

struct StepModel {
    config: StepConfig,
    prompt: Option<String>,
    summary: Option<String>,
    cycle_time: Option<String>,
    trajectories: Vec<Trajectory>,
}

struct SummaryStrip {
    total_steps: usize,
    completed_steps: usize,
    in_progress_steps: usize,
    blocked_steps: usize,
    pending_steps: usize,
    archived_sagas: usize,
}

fn build_view_model(saga_path: &Path) -> Result<ViewModel> {
    let current = load_current(saga_path).ok().flatten();
    let archives = load_archives(saga_path)?;
    let briefing = instructions::freshness_warning(saga_path).ok().flatten();
    let github_repo = detect_github_repo(saga_path);

    let mut total_steps = 0usize;
    let mut completed = 0usize;
    let mut in_progress = 0usize;
    let mut blocked = 0usize;
    let mut pending = 0usize;
    let count_in = |steps: &[StepModel],
                    t: &mut usize,
                    c: &mut usize,
                    ip: &mut usize,
                    b: &mut usize,
                    p: &mut usize| {
        for s in steps {
            *t += 1;
            match s.config.status {
                StepStatus::Completed => *c += 1,
                StepStatus::InProgress => *ip += 1,
                StepStatus::Blocked => *b += 1,
                StepStatus::Pending => *p += 1,
            }
        }
    };
    if let Some(ref c) = current {
        count_in(
            &c.steps,
            &mut total_steps,
            &mut completed,
            &mut in_progress,
            &mut blocked,
            &mut pending,
        );
    }
    for a in &archives {
        count_in(
            &a.steps,
            &mut total_steps,
            &mut completed,
            &mut in_progress,
            &mut blocked,
            &mut pending,
        );
    }

    Ok(ViewModel {
        saga_path: saga_path.to_path_buf(),
        current,
        summary: SummaryStrip {
            total_steps,
            completed_steps: completed,
            in_progress_steps: in_progress,
            blocked_steps: blocked,
            pending_steps: pending,
            archived_sagas: archives.len(),
        },
        archives,
        briefing,
        github_repo,
    })
}

fn load_current(saga_path: &Path) -> Result<Option<CurrentSagaModel>> {
    if !saga::saga_exists(saga_path) {
        return Ok(None);
    }
    let config = saga::load_saga(saga_path)?;
    let saga_dir = saga::saga_dir(saga_path);
    let plan_path = saga_path.join(&config.plan_file);
    let plan = std::fs::read_to_string(&plan_path).unwrap_or_default();
    let raw_steps = step::list_steps(&saga_dir)?;
    let steps = build_step_models(&saga_dir, raw_steps)?;
    let pct = pct_complete(&steps);
    Ok(Some(CurrentSagaModel {
        config,
        plan,
        steps,
        pct_complete: pct,
    }))
}

fn load_archives(saga_path: &Path) -> Result<Vec<ArchiveSagaModel>> {
    let raw = archive::list_archives(saga_path)?;
    let mut out = Vec::with_capacity(raw.len());
    for a in raw {
        let raw_steps = step::list_steps(&a.dir).unwrap_or_default();
        let steps = build_step_models(&a.dir, raw_steps)?;
        let pct = pct_complete(&steps);
        out.push(ArchiveSagaModel {
            config: a.config,
            suffix: a.suffix,
            reason: a.reason,
            steps,
            pct_complete: pct,
        });
    }
    Ok(out)
}

fn build_step_models(
    saga_dir: &Path,
    raw: Vec<(PathBuf, StepConfig)>,
) -> Result<Vec<StepModel>> {
    // Pre-load trajectories per task_type once so each step doesn't re-read.
    let mut tj_cache: HashMap<String, Vec<Trajectory>> = HashMap::new();
    let traj_root = saga_dir.join("trajectories");
    if traj_root.is_dir() {
        for entry in std::fs::read_dir(&traj_root)? {
            let entry = entry?;
            let p = entry.path();
            if !p.is_dir() {
                continue;
            }
            let name = match p.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            let mut tjs = trajectory::load_all_trajectories(&p).unwrap_or_default();
            tjs.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
            tj_cache.insert(name, tjs);
        }
    }

    let mut out = Vec::with_capacity(raw.len());
    for (dir, config) in raw {
        let prompt = std::fs::read_to_string(dir.join("prompt.md")).ok();
        let summary = std::fs::read_to_string(dir.join("summary.md")).ok();
        let cycle_time = compute_cycle_time(&config);
        let trajectories = config
            .task_type
            .as_ref()
            .and_then(|t| tj_cache.get(t).cloned())
            .unwrap_or_default();
        out.push(StepModel {
            config,
            prompt,
            summary,
            cycle_time,
            trajectories,
        });
    }
    Ok(out)
}

fn compute_cycle_time(c: &StepConfig) -> Option<String> {
    let completed = c.completed_at.as_deref()?;
    use chrono::NaiveDateTime;
    let start = NaiveDateTime::parse_from_str(&c.created_at, "%Y-%m-%dT%H:%M:%S").ok()?;
    let end = NaiveDateTime::parse_from_str(completed, "%Y-%m-%dT%H:%M:%S").ok()?;
    let secs = (end - start).num_seconds();
    Some(humanize_secs(secs))
}

fn humanize_secs(s: i64) -> String {
    if s < 0 {
        return format!("{s}s");
    }
    if s < 60 {
        return format!("{s}s");
    }
    if s < 3600 {
        return format!("{}m{}s", s / 60, s % 60);
    }
    if s < 86400 {
        return format!("{}h{}m", s / 3600, (s % 3600) / 60);
    }
    format!("{}d{}h", s / 86400, (s % 86400) / 3600)
}

fn pct_complete(steps: &[StepModel]) -> u32 {
    if steps.is_empty() {
        return 0;
    }
    let total = steps.len() as f32;
    let mut credit = 0.0f32;
    for s in steps {
        match s.config.status {
            StepStatus::Completed => credit += 1.0,
            StepStatus::InProgress => credit += 0.5,
            _ => {}
        }
    }
    (credit / total * 100.0).round() as u32
}

fn detect_github_repo(saga_path: &Path) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(saga_path)
        .arg("remote")
        .arg("get-url")
        .arg("origin")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let url = String::from_utf8(out.stdout).ok()?.trim().to_string();
    // git@github.com:org/repo.git OR https://github.com/org/repo[.git]
    let stripped = url
        .strip_prefix("git@github.com:")
        .or_else(|| url.strip_prefix("https://github.com/"))?
        .strip_suffix(".git")
        .unwrap_or_else(|| {
            url.strip_prefix("git@github.com:")
                .or_else(|| url.strip_prefix("https://github.com/"))
                .unwrap_or("")
        });
    if stripped.is_empty() {
        return None;
    }
    Some(stripped.to_string())
}

// ---------------------------------------------------------------------------
// HTML rendering
// ---------------------------------------------------------------------------

fn render_html(m: &ViewModel) -> String {
    let title = m
        .current
        .as_ref()
        .map(|c| c.config.name.clone())
        .unwrap_or_else(|| "agentrail".to_string());
    let header = render_header(m, &title);
    let status_tab = render_status_tab(m);
    let history_tab = render_history_tab(m);
    let plans_tab = render_plans_tab(m);

    format!(
        "<!DOCTYPE html>\n\
         <html lang=\"en\">\n\
         <head>\n\
           <meta charset=\"utf-8\">\n\
           <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
           <title>{title} — agentrail view</title>\n\
           <style>{css}</style>\n\
         </head>\n\
         <body>\n\
         {header}\
         <nav class=\"tabs\" role=\"tablist\">\n\
           <button class=\"tab-btn active\" data-tab=\"status\" role=\"tab\">Status</button>\n\
           <button class=\"tab-btn\" data-tab=\"history\" role=\"tab\">History</button>\n\
           <button class=\"tab-btn\" data-tab=\"plans\" role=\"tab\">Plans</button>\n\
         </nav>\n\
         <main>\n\
           <section id=\"status\" class=\"tab active\" role=\"tabpanel\">{status_tab}</section>\n\
           <section id=\"history\" class=\"tab\" role=\"tabpanel\">{history_tab}</section>\n\
           <section id=\"plans\" class=\"tab\" role=\"tabpanel\">{plans_tab}</section>\n\
         </main>\n\
         <footer><small>Generated by agentrail view from {saga_path}.</small></footer>\n\
         <script>{js}</script>\n\
         </body>\n\
         </html>\n",
        title = esc(&title),
        css = CSS,
        js = JS,
        saga_path = esc(&m.saga_path.display().to_string()),
    )
}

fn render_header(m: &ViewModel, title: &str) -> String {
    let s = &m.summary;
    let briefing_html = match &m.briefing {
        Some(msg) => format!(
            "<div class=\"briefing-status stale\"><strong>Briefing:</strong> stale<details><summary>details</summary><pre>{}</pre></details></div>",
            esc(msg)
        ),
        None => "<div class=\"briefing-status ok\"><strong>Briefing:</strong> up to date or not configured</div>".to_string(),
    };
    format!(
        "<header>\n\
           <h1>{title}</h1>\n\
           <div class=\"summary-strip\">\n\
             <div class=\"metric\"><span class=\"label\">Steps</span><span class=\"value\">{total}</span></div>\n\
             <div class=\"metric\"><span class=\"label\">Done</span><span class=\"value good\">{done}</span></div>\n\
             <div class=\"metric\"><span class=\"label\">In progress</span><span class=\"value\">{ip}</span></div>\n\
             <div class=\"metric\"><span class=\"label\">Blocked</span><span class=\"value bad\">{bl}</span></div>\n\
             <div class=\"metric\"><span class=\"label\">Pending</span><span class=\"value\">{pe}</span></div>\n\
             <div class=\"metric\"><span class=\"label\">Archives</span><span class=\"value\">{ar}</span></div>\n\
           </div>\n\
           {briefing_html}\n\
         </header>\n",
        title = esc(title),
        total = s.total_steps,
        done = s.completed_steps,
        ip = s.in_progress_steps,
        bl = s.blocked_steps,
        pe = s.pending_steps,
        ar = s.archived_sagas,
    )
}

fn render_status_tab(m: &ViewModel) -> String {
    match &m.current {
        None => "<p class=\"empty\">No active saga. Run <code>agentrail init</code> to start one.</p>".to_string(),
        Some(c) => {
            let header = format!(
                "<div class=\"saga-card\">\n\
                   <h2>{name} <span class=\"status-pill {sclass}\">{status}</span></h2>\n\
                   <div class=\"meta\">Created {created} · Current step {cur}{retro}</div>\n\
                   {progress}\n\
                   <details><summary>Plan</summary><pre class=\"plan\">{plan}</pre></details>\n\
                 </div>\n",
                name = esc(&c.config.name),
                sclass = match c.config.status { SagaStatus::Active => "active", SagaStatus::Completed => "completed" },
                status = c.config.status,
                created = esc(&c.config.created_at),
                cur = c.config.current_step,
                retro = if c.config.retroactive { " · <em>retroactive</em>" } else { "" },
                progress = render_progress(c.pct_complete),
                plan = esc(&c.plan),
            );
            let steps = render_steps(&c.steps, m.github_repo.as_deref(), c.config.current_step);
            format!("{header}<h3>Steps</h3>{steps}")
        }
    }
}

fn render_history_tab(m: &ViewModel) -> String {
    if m.archives.is_empty() {
        return "<p class=\"empty\">No archived sagas yet.</p>".to_string();
    }
    let mut out = String::new();
    for a in &m.archives {
        out.push_str(&format!(
            "<details class=\"saga-card archive\">\n\
               <summary>\n\
                 <span class=\"saga-name\">{name}</span>\n\
                 <span class=\"saga-suffix\">{suffix}</span>\n\
                 <span class=\"status-pill {sclass}\">{status}</span>\n\
                 {progress}\n\
               </summary>\n\
               <div class=\"meta\">Created {created}{reason}</div>\n\
               {steps}\n\
             </details>\n",
            name = esc(&a.config.name),
            suffix = esc(&a.suffix),
            sclass = match a.config.status {
                SagaStatus::Active => "active",
                SagaStatus::Completed => "completed",
            },
            status = a.config.status,
            progress = render_progress_inline(a.pct_complete),
            created = esc(&a.config.created_at),
            reason = a
                .reason
                .as_ref()
                .map(|r| format!(" · reason: {}", esc(r)))
                .unwrap_or_default(),
            steps = render_steps(&a.steps, m.github_repo.as_deref(), 0),
        ));
    }
    out
}

fn render_plans_tab(m: &ViewModel) -> String {
    let Some(c) = &m.current else {
        return "<p class=\"empty\">No active saga; no plans to show.</p>".to_string();
    };
    let pending: Vec<&StepModel> = c
        .steps
        .iter()
        .filter(|s| s.config.status == StepStatus::Pending)
        .collect();
    if pending.is_empty() {
        return "<p class=\"empty\">No pending steps. The current saga has nothing planned ahead of it.</p>".to_string();
    }
    let lead = format!(
        "<p class=\"plans-lead\">{n} pending step{plural}, ahead of cursor at step {cur}.</p>",
        n = pending.len(),
        plural = if pending.len() == 1 { "" } else { "s" },
        cur = c.config.current_step,
    );
    let mut steps = String::new();
    for s in pending {
        steps.push_str(&render_step_card(s, m.github_repo.as_deref(), false));
    }
    format!("{lead}{steps}")
}

fn render_steps(steps: &[StepModel], github_repo: Option<&str>, current_step: u32) -> String {
    if steps.is_empty() {
        return "<p class=\"empty\">No steps yet.</p>".to_string();
    }
    let mut out = String::new();
    for s in steps {
        let is_current = s.config.number == current_step && current_step != 0;
        out.push_str(&render_step_card(s, github_repo, is_current));
    }
    out
}

fn render_step_card(s: &StepModel, github_repo: Option<&str>, is_current: bool) -> String {
    let status_class = match s.config.status {
        StepStatus::Completed => "completed",
        StepStatus::InProgress => "in-progress",
        StepStatus::Blocked => "blocked",
        StepStatus::Pending => "pending",
    };
    let cycle = s
        .cycle_time
        .as_ref()
        .map(|t| format!("<span class=\"cycle\">{}</span>", esc(t)))
        .unwrap_or_default();
    let task_type = s
        .config
        .task_type
        .as_ref()
        .map(|t| format!("<span class=\"task-type\">{}</span>", esc(t)))
        .unwrap_or_default();
    let here = if is_current {
        " <span class=\"here-marker\">← cursor</span>"
    } else {
        ""
    };

    let prompt_html = match &s.prompt {
        Some(p) if !p.trim().is_empty() => format!(
            "<details><summary>Prompt</summary><pre>{}</pre></details>",
            esc(p)
        ),
        _ => String::new(),
    };
    let summary_html = match &s.summary {
        Some(p) if !p.trim().is_empty() => format!(
            "<details><summary>Summary</summary><pre>{}</pre></details>",
            esc(p)
        ),
        _ => String::new(),
    };
    let commits_html = render_commits(&s.config.commits, github_repo);
    let trajectories_html = render_trajectories(&s.trajectories);

    format!(
        "<details class=\"step status-{status_class}\"{open_attr}>\n\
           <summary>\n\
             <span class=\"step-num\">{num:03}</span>\n\
             <span class=\"step-slug\">{slug}</span>\n\
             <span class=\"status-pill {status_class}\">{status}</span>\n\
             {task_type}\n\
             <span class=\"step-desc\">{desc}</span>\n\
             {cycle}\n\
             {here}\n\
           </summary>\n\
           <div class=\"step-detail\">\n\
             <div class=\"meta\">role: {role} · created {created}{completed}</div>\n\
             {prompt_html}\n\
             {summary_html}\n\
             {commits_html}\n\
             {trajectories_html}\n\
           </div>\n\
         </details>\n",
        open_attr = if is_current || s.config.status == StepStatus::Blocked {
            " open"
        } else {
            ""
        },
        num = s.config.number,
        slug = esc(&s.config.slug),
        status = s.config.status,
        desc = esc(&s.config.description),
        role = s.config.role,
        created = esc(&s.config.created_at),
        completed = s
            .config
            .completed_at
            .as_ref()
            .map(|t| format!(" · completed {}", esc(t)))
            .unwrap_or_default(),
    )
}

fn render_commits(commits: &[String], github_repo: Option<&str>) -> String {
    if commits.is_empty() {
        return String::new();
    }
    let mut items = String::new();
    for sha in commits {
        let short: String = sha.chars().take(12).collect();
        let item = match github_repo {
            Some(repo) => format!(
                "<li><a href=\"https://github.com/{repo}/commit/{sha}\" target=\"_blank\" rel=\"noopener\"><code>{short}</code></a></li>",
                repo = esc(repo),
                sha = esc(sha),
                short = esc(&short)
            ),
            None => format!("<li><code>{}</code></li>", esc(&short)),
        };
        items.push_str(&item);
    }
    format!("<details><summary>Commits ({})</summary><ul class=\"commits\">{}</ul></details>", commits.len(), items)
}

fn render_trajectories(tjs: &[Trajectory]) -> String {
    if tjs.is_empty() {
        return String::new();
    }
    let n_success = tjs.iter().filter(|t| t.reward > 0).count();
    let n_failure = tjs.iter().filter(|t| t.reward < 0).count();
    let mut rows = String::new();
    for t in tjs {
        let cls = if t.reward > 0 {
            "good"
        } else if t.reward < 0 {
            "bad"
        } else {
            "neutral"
        };
        rows.push_str(&format!(
            "<tr class=\"{cls}\"><td>{ts}</td><td>{action}</td><td>{result}</td><td class=\"reward\">{reward:+}</td></tr>\n",
            ts = esc(&t.timestamp),
            action = esc(&t.action),
            result = esc(&t.result),
            reward = t.reward,
        ));
    }
    format!(
        "<details><summary>Trajectories ({n} · ✓{ok} · ✗{bad})</summary>\n\
         <table class=\"trajectories\"><thead><tr><th>time</th><th>action</th><th>result</th><th>reward</th></tr></thead><tbody>{rows}</tbody></table>\n\
         </details>",
        n = tjs.len(),
        ok = n_success,
        bad = n_failure,
    )
}

fn render_progress(pct: u32) -> String {
    format!(
        "<div class=\"progress\" role=\"progressbar\" aria-valuenow=\"{pct}\" aria-valuemin=\"0\" aria-valuemax=\"100\">\n\
           <div class=\"bar\" style=\"width:{pct}%\"></div>\n\
           <span class=\"pct\">{pct}% complete</span>\n\
         </div>",
    )
}

fn render_progress_inline(pct: u32) -> String {
    format!(
        "<span class=\"progress-inline\"><span class=\"bar\" style=\"width:{pct}%\"></span><span class=\"pct\">{pct}%</span></span>",
    )
}

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Static assets — single CSS string + tiny JS for tab switching.
// ---------------------------------------------------------------------------

const CSS: &str = r#"
:root {
  --bg: #fafaf9;
  --fg: #1c1917;
  --muted: #57534e;
  --border: #d6d3d1;
  --card: #ffffff;
  --accent: #0369a1;
  --good: #16a34a;
  --warn: #d97706;
  --bad: #dc2626;
  --neutral: #6b7280;
  --code-bg: #f4f4f5;
}
@media (prefers-color-scheme: dark) {
  :root {
    --bg: #0c0a09;
    --fg: #e7e5e4;
    --muted: #a8a29e;
    --border: #292524;
    --card: #1c1917;
    --accent: #38bdf8;
    --good: #4ade80;
    --warn: #fbbf24;
    --bad: #f87171;
    --neutral: #94a3b8;
    --code-bg: #0a0a0a;
  }
}
* { box-sizing: border-box; }
body { margin: 0; font: 14px/1.5 -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif; color: var(--fg); background: var(--bg); }
header { padding: 1.25rem 1.5rem; border-bottom: 1px solid var(--border); }
header h1 { margin: 0 0 0.75rem 0; font-size: 1.5rem; }
.summary-strip { display: flex; gap: 1.25rem; flex-wrap: wrap; margin-bottom: 0.75rem; }
.metric { display: flex; flex-direction: column; line-height: 1.2; }
.metric .label { font-size: 0.75rem; color: var(--muted); text-transform: uppercase; letter-spacing: 0.04em; }
.metric .value { font-size: 1.5rem; font-weight: 600; }
.metric .value.good { color: var(--good); }
.metric .value.bad { color: var(--bad); }
.briefing-status { font-size: 0.85rem; padding: 0.5rem 0.75rem; border-radius: 6px; background: var(--code-bg); border: 1px solid var(--border); }
.briefing-status.stale { border-color: var(--warn); color: var(--warn); }
.briefing-status pre { white-space: pre-wrap; margin: 0.5rem 0 0; font-size: 0.8rem; }
nav.tabs { display: flex; gap: 0.5rem; padding: 0 1.5rem; border-bottom: 1px solid var(--border); background: var(--bg); position: sticky; top: 0; z-index: 10; }
.tab-btn { background: transparent; color: var(--muted); border: none; border-bottom: 2px solid transparent; padding: 0.75rem 1rem; font: inherit; font-weight: 500; cursor: pointer; }
.tab-btn:hover { color: var(--fg); }
.tab-btn.active { color: var(--accent); border-bottom-color: var(--accent); }
main { padding: 1.5rem; }
.tab { display: none; }
.tab.active { display: block; }
.empty { color: var(--muted); padding: 2rem 0; text-align: center; }
.saga-card { background: var(--card); border: 1px solid var(--border); border-radius: 8px; padding: 1rem 1.25rem; margin-bottom: 1rem; }
.saga-card h2 { margin: 0 0 0.5rem; font-size: 1.2rem; display: flex; align-items: center; gap: 0.5rem; }
.saga-card.archive { padding: 0; }
.saga-card.archive > summary { padding: 0.75rem 1rem; cursor: pointer; display: flex; gap: 0.75rem; align-items: center; }
.saga-card.archive > summary:hover { background: var(--code-bg); }
.saga-card.archive[open] > summary { border-bottom: 1px solid var(--border); }
.saga-card.archive > summary::-webkit-details-marker { display: none; }
.saga-card.archive > summary::marker { content: ""; }
.saga-card.archive .meta, .saga-card.archive .step { padding-left: 1rem; padding-right: 1rem; }
.saga-suffix { font-family: ui-monospace, monospace; font-size: 0.85rem; color: var(--muted); }
.meta { color: var(--muted); font-size: 0.85rem; margin-bottom: 0.5rem; }
.status-pill { display: inline-block; font-size: 0.7rem; padding: 0.15rem 0.55rem; border-radius: 999px; text-transform: lowercase; font-weight: 600; letter-spacing: 0.02em; }
.status-pill.completed { background: color-mix(in srgb, var(--good) 20%, transparent); color: var(--good); }
.status-pill.in-progress { background: color-mix(in srgb, var(--accent) 20%, transparent); color: var(--accent); }
.status-pill.blocked { background: color-mix(in srgb, var(--bad) 20%, transparent); color: var(--bad); }
.status-pill.pending { background: color-mix(in srgb, var(--muted) 20%, transparent); color: var(--muted); }
.status-pill.active { background: color-mix(in srgb, var(--accent) 20%, transparent); color: var(--accent); }
.progress { background: var(--code-bg); height: 1.5rem; border-radius: 6px; position: relative; overflow: hidden; margin: 0.5rem 0; border: 1px solid var(--border); }
.progress .bar { background: var(--accent); height: 100%; transition: width 0.2s; }
.progress .pct { position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; font-size: 0.8rem; font-weight: 600; }
.progress-inline { display: inline-flex; align-items: center; gap: 0.5rem; min-width: 8rem; margin-left: auto; }
.progress-inline .bar { background: var(--accent); height: 0.5rem; border-radius: 3px; flex: 1; max-width: 6rem; }
.progress-inline .pct { font-size: 0.8rem; color: var(--muted); }
details { margin-bottom: 0.5rem; }
details > summary { cursor: pointer; padding: 0.4rem 0.5rem; border-radius: 4px; }
details > summary:hover { background: var(--code-bg); }
.step { background: var(--card); border: 1px solid var(--border); border-radius: 6px; margin-bottom: 0.5rem; }
.step > summary { display: flex; gap: 0.6rem; align-items: center; flex-wrap: wrap; padding: 0.5rem 0.75rem; }
.step.status-blocked > summary { background: color-mix(in srgb, var(--bad) 6%, transparent); }
.step.status-in-progress > summary { background: color-mix(in srgb, var(--accent) 6%, transparent); }
.step-num { font-family: ui-monospace, monospace; color: var(--muted); font-size: 0.8rem; min-width: 2.5rem; }
.step-slug { font-family: ui-monospace, monospace; font-size: 0.85rem; }
.step-desc { color: var(--fg); }
.task-type { font-size: 0.7rem; padding: 0.1rem 0.4rem; border-radius: 4px; background: var(--code-bg); color: var(--muted); font-family: ui-monospace, monospace; }
.cycle { color: var(--muted); font-size: 0.8rem; margin-left: auto; }
.here-marker { color: var(--accent); font-size: 0.75rem; font-weight: 600; }
.step-detail { padding: 0.5rem 1rem 1rem; }
.step-detail > details { margin-top: 0.5rem; }
pre { background: var(--code-bg); border: 1px solid var(--border); padding: 0.75rem; border-radius: 4px; white-space: pre-wrap; word-wrap: break-word; font-size: 0.85rem; overflow-x: auto; }
pre.plan { max-height: 24rem; overflow-y: auto; }
.commits { margin: 0.25rem 0 0 1.25rem; padding: 0; }
.commits li { font-family: ui-monospace, monospace; font-size: 0.85rem; }
.commits a { color: var(--accent); text-decoration: none; }
.commits a:hover { text-decoration: underline; }
.trajectories { width: 100%; border-collapse: collapse; font-size: 0.85rem; margin-top: 0.5rem; }
.trajectories th, .trajectories td { text-align: left; padding: 0.3rem 0.5rem; border-bottom: 1px solid var(--border); }
.trajectories th { color: var(--muted); font-weight: 500; font-size: 0.75rem; text-transform: uppercase; }
.trajectories tr.good td.reward { color: var(--good); }
.trajectories tr.bad td.reward { color: var(--bad); }
.trajectories tr.neutral td.reward { color: var(--neutral); }
.plans-lead { color: var(--muted); margin-bottom: 1rem; }
footer { padding: 2rem 1.5rem; color: var(--muted); border-top: 1px solid var(--border); margin-top: 2rem; }
"#;

const JS: &str = r#"
document.addEventListener('DOMContentLoaded', () => {
  const tabs = document.querySelectorAll('.tab-btn');
  const panes = document.querySelectorAll('.tab');
  function activate(name) {
    tabs.forEach(b => b.classList.toggle('active', b.dataset.tab === name));
    panes.forEach(p => p.classList.toggle('active', p.id === name));
    if (history.replaceState) history.replaceState(null, '', '#' + name);
  }
  tabs.forEach(b => b.addEventListener('click', () => activate(b.dataset.tab)));
  // Initial selection from #hash if valid.
  const initial = (location.hash || '').replace('#', '');
  if (['status', 'history', 'plans'].includes(initial)) activate(initial);
  // Keyboard: 1/2/3 to switch tabs.
  document.addEventListener('keydown', (e) => {
    if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA') return;
    if (e.key === '1') activate('status');
    if (e.key === '2') activate('history');
    if (e.key === '3') activate('plans');
  });
});
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn html_escape_handles_all_metachars() {
        assert_eq!(esc("<a&b>\"c'd"), "&lt;a&amp;b&gt;&quot;c&#39;d");
    }

    #[test]
    fn humanize_secs_buckets() {
        assert_eq!(humanize_secs(45), "45s");
        assert_eq!(humanize_secs(125), "2m5s");
        assert_eq!(humanize_secs(3700), "1h1m");
        assert_eq!(humanize_secs(90000), "1d1h");
    }

    #[test]
    fn pct_complete_handles_mixed_states() {
        let mk = |status: StepStatus| StepModel {
            config: StepConfig {
                number: 1,
                slug: "x".into(),
                status,
                description: "x".into(),
                role: agentrail_core::StepRole::Production,
                context_files: vec![],
                created_at: "2026-01-01T00:00:00".into(),
                completed_at: None,
                transcript_file: None,
                job_spec: None,
                packet_file: None,
                task_type: None,
                commits: vec![],
            },
            prompt: None,
            summary: None,
            cycle_time: None,
            trajectories: vec![],
        };
        let steps = vec![
            mk(StepStatus::Completed),
            mk(StepStatus::Completed),
            mk(StepStatus::InProgress),
            mk(StepStatus::Pending),
        ];
        // 2 complete + 0.5 for in-progress = 2.5 / 4 = 62.5 → rounds to 63
        assert_eq!(pct_complete(&steps), 63);
        assert_eq!(pct_complete(&[]), 0);
    }

    #[test]
    fn view_writes_html_with_all_three_tabs() {
        let tmp = tempdir().unwrap();
        // Bare repo (no saga). View should still emit an HTML page.
        let args = ViewArgs {
            output: None,
            no_open: true,
        };
        run(tmp.path(), &args).unwrap();
        let path = tmp.path().join("agentrail-view.html");
        assert!(path.is_file());
        let html = std::fs::read_to_string(&path).unwrap();
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("id=\"status\""));
        assert!(html.contains("id=\"history\""));
        assert!(html.contains("id=\"plans\""));
        assert!(html.contains("No active saga"));
    }

    #[test]
    fn view_with_active_saga_renders_steps_and_progress() {
        let tmp = tempdir().unwrap();
        saga::init_saga(tmp.path(), "demo", "# Plan\nDo things").unwrap();
        let saga_dir = saga::saga_dir(tmp.path());
        // Create one step (Pending), then transition it to Completed.
        let dir1 = step::create_step(&step::CreateStepParams {
            saga_dir: &saga_dir,
            number: 1,
            slug: "first",
            prompt: "first prompt",
            description: "First step",
            role: agentrail_core::StepRole::Production,
            context_files: &[],
            task_type: None,
            job_spec: None,
        })
        .unwrap();
        let mut s1 = step::load_step(&dir1).unwrap();
        s1.status = StepStatus::Completed;
        s1.completed_at = Some("2026-01-01T01:00:00".into());
        s1.created_at = "2026-01-01T00:00:00".into();
        step::save_step(&dir1, &s1).unwrap();

        // Second step stays Pending.
        step::create_step(&step::CreateStepParams {
            saga_dir: &saga_dir,
            number: 2,
            slug: "second",
            prompt: "second prompt",
            description: "Second step",
            role: agentrail_core::StepRole::Production,
            context_files: &[],
            task_type: None,
            job_spec: None,
        })
        .unwrap();

        let args = ViewArgs {
            output: None,
            no_open: true,
        };
        run(tmp.path(), &args).unwrap();
        let html =
            std::fs::read_to_string(tmp.path().join(".agentrail/view.html")).unwrap();
        assert!(html.contains("demo"));
        assert!(html.contains("first"));
        assert!(html.contains("second"));
        // 1/2 done, 0 in progress → 50%
        assert!(html.contains("50%"));
        // Plans tab should mention pending count
        assert!(html.contains("1 pending step"));
    }

    #[test]
    fn view_includes_archived_saga_in_history_tab() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join(".agentrail-archive/old-20260101T000000")).unwrap();
        std::fs::write(
            root.join(".agentrail-archive/old-20260101T000000/saga.toml"),
            "name = \"old\"\nstatus = \"completed\"\ncurrent_step = 0\ncreated_at = \"2026-01-01\"\nplan_file = \".agentrail/plan.md\"\nretroactive = false\n",
        )
        .unwrap();

        let args = ViewArgs {
            output: None,
            no_open: true,
        };
        run(root, &args).unwrap();
        let html = std::fs::read_to_string(root.join("agentrail-view.html")).unwrap();
        assert!(html.contains("\"old\"") || html.contains(">old<"));
        assert!(html.contains("20260101T000000"));
    }
}
