//! Step transcript: the small, committable digest of a session log.
//!
//! The full session log is kept outside the repository (see
//! [`crate::session`]) because it is large and quotes everything the
//! agent read. What a reviewer wants from it is far smaller: what the
//! user asked and what the assistant answered. `agentrail complete`
//! writes that into `steps/NNN-slug/transcript.md` — prompts and replies
//! only, with no tool calls, tool results, thinking or metadata — plus
//! the step's commits, a token count, and the SHA-256 of the full log so
//! the copy in the session (or shadow) store can be matched to it.

use agentrail_core::error::Result;
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use std::collections::HashMap;
use std::path::Path;

pub const TRANSCRIPT_FILE: &str = "transcript.md";

/// Wrapper blocks the harness injects into user records. They are not
/// something the user typed, and reminders can quote file contents.
const INJECTED_BLOCKS: [&str; 4] = [
    "system-reminder",
    "local-command-caveat",
    "local-command-stdout",
    "command-message",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turn {
    pub role: Role,
    pub timestamp: String,
    pub text: String,
}

/// Token usage summed over the distinct API responses in a line range.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

/// The digest of one line range of a session log.
#[derive(Debug, Clone, Default)]
pub struct Digest {
    pub turns: Vec<Turn>,
    pub tokens: Tokens,
}

/// Digest lines `[start, start + count)` (0-based) of a session log.
pub fn digest_lines(log: &str, start: usize, count: usize) -> Digest {
    let mut turns: Vec<Turn> = Vec::new();
    // One API response is logged as several records (one per content
    // block), each repeating the same usage; count each response once.
    let mut usage: HashMap<String, Tokens> = HashMap::new();
    let mut anonymous = 0usize;

    for line in log.lines().skip(start).take(count) {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v.get("isSidechain").and_then(Value::as_bool) == Some(true)
            || v.get("isMeta").and_then(Value::as_bool) == Some(true)
        {
            continue;
        }
        let role = match v.get("type").and_then(Value::as_str) {
            Some("user") => Role::User,
            Some("assistant") => Role::Assistant,
            _ => continue,
        };
        let Some(msg) = v.get("message") else {
            continue;
        };

        if role == Role::Assistant
            && let Some(u) = msg.get("usage")
        {
            let n = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
            let key = match msg.get("id").and_then(Value::as_str) {
                Some(id) => id.to_string(),
                None => {
                    anonymous += 1;
                    format!("anonymous-{anonymous}")
                }
            };
            usage.insert(
                key,
                Tokens {
                    input: n("input_tokens"),
                    output: n("output_tokens"),
                    cache_read: n("cache_read_input_tokens"),
                    cache_write: n("cache_creation_input_tokens"),
                },
            );
        }

        let text = match role {
            Role::User => strip_injected(&text_blocks(msg)),
            Role::Assistant => text_blocks(msg),
        };
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let timestamp = v
            .get("timestamp")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        match turns.last_mut() {
            // Consecutive records of one speaker are one turn.
            Some(last) if last.role == role => {
                last.text.push_str("\n\n");
                last.text.push_str(text);
            }
            _ => turns.push(Turn {
                role,
                timestamp,
                text: text.to_string(),
            }),
        }
    }

    let mut tokens = Tokens::default();
    for t in usage.values() {
        tokens.input += t.input;
        tokens.output += t.output;
        tokens.cache_read += t.cache_read;
        tokens.cache_write += t.cache_write;
    }
    Digest { turns, tokens }
}

/// The `text` content of a message: a plain string, or the `text` blocks
/// of a content array. `tool_use`, `tool_result` and `thinking` blocks
/// are dropped.
fn text_blocks(msg: &Value) -> String {
    match msg.get("content") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .filter(|i| i.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|i| i.get("text").and_then(Value::as_str))
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n"),
        _ => String::new(),
    }
}

/// Remove harness-injected `<tag>...</tag>` blocks from a user message.
fn strip_injected(text: &str) -> String {
    let mut out = text.to_string();
    for tag in INJECTED_BLOCKS {
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        while let Some(start) = out.find(&open) {
            match out[start..].find(&close) {
                Some(rel) => out.replace_range(start..start + rel + close.len(), ""),
                // Unterminated: drop the rest rather than leak it.
                None => out.truncate(start),
            }
        }
    }
    out
}

/// SHA-256 of a file, as lowercase hex.
pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// What the transcript header says about where the digest came from.
pub struct Source<'a> {
    /// Step label, e.g. `003-tune-kernel`.
    pub step: &'a str,
    /// Session id (the log's file stem).
    pub session: &'a str,
    /// First log line of this step, 0-based.
    pub first_line: usize,
    /// Lines this step added.
    pub line_count: usize,
    /// SHA-256 of the log's first `first_line + line_count` lines.
    pub log_sha256: &'a str,
    pub commits: &'a [String],
}

/// Render `transcript.md`.
pub fn render(src: &Source<'_>, digest: &Digest) -> String {
    let end = src.first_line + src.line_count;
    let mut out = format!("# Transcript: step {}\n\n", src.step);
    out.push_str(&format!("- session: `{}`\n", src.session));
    out.push_str(&format!(
        "- log lines: {}–{} ({} lines)\n",
        src.first_line + 1,
        end,
        src.line_count
    ));
    out.push_str(&format!(
        "- log sha256: `{}` (the log's first {} lines)\n",
        src.log_sha256, end
    ));
    if src.commits.is_empty() {
        out.push_str("- commits: none recorded\n");
    } else {
        out.push_str(&format!("- commits: {}\n", src.commits.join(", ")));
    }
    let t = &digest.tokens;
    out.push_str(&format!(
        "- tokens: {} input, {} output, {} cache read, {} cache write\n",
        t.input, t.output, t.cache_read, t.cache_write
    ));
    out.push_str(
        "\nPrompts and replies only. Tool calls, tool results, thinking and metadata are\nomitted; the full log is kept outside the repository (`agentrail sessions path`).\n",
    );
    for turn in &digest.turns {
        let who = match turn.role {
            Role::User => "User",
            Role::Assistant => "Assistant",
        };
        out.push_str("\n---\n\n");
        if turn.timestamp.is_empty() {
            out.push_str(&format!("**{who}**\n\n"));
        } else {
            out.push_str(&format!("**{who}** · {}\n\n", turn.timestamp));
        }
        out.push_str(&turn.text);
        out.push('\n');
    }
    out
}

/// Write the transcript for the lines a snapshot just added, into
/// `step_dir`. Returns `None` (and writes nothing) when the snapshot
/// added no lines or the lines hold no conversation.
pub fn write_step_transcript(
    step_dir: &Path,
    step_label: &str,
    snapshot: &crate::session::Snapshot,
    commits: &[String],
) -> Result<Option<std::path::PathBuf>> {
    if snapshot.new_lines == 0 {
        return Ok(None);
    }
    let log = std::fs::read_to_string(&snapshot.path)?;
    let digest = digest_lines(&log, snapshot.first_new_line, snapshot.new_lines);
    if digest.turns.is_empty() {
        return Ok(None);
    }
    let session = snapshot
        .path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let sha = sha256_file(&snapshot.path)?;
    let body = render(
        &Source {
            step: step_label,
            session: &session,
            first_line: snapshot.first_new_line,
            line_count: snapshot.new_lines,
            log_sha256: &sha,
            commits,
        },
        &digest,
    );
    let path = step_dir.join(TRANSCRIPT_FILE);
    std::fs::write(&path, body)?;
    Ok(Some(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn log(records: &[Value]) -> String {
        records.iter().map(|r| format!("{r}\n")).collect::<String>()
    }

    fn user(text: &str) -> Value {
        json!({"type":"user","timestamp":"T1","message":{"role":"user","content":text}})
    }

    fn assistant(id: &str, block: Value, out_tokens: u64) -> Value {
        json!({"type":"assistant","timestamp":"T2","message":{
            "id": id, "content":[block],
            "usage":{"input_tokens":2,"output_tokens":out_tokens,
                     "cache_read_input_tokens":100,"cache_creation_input_tokens":10}}})
    }

    #[test]
    fn keeps_prompts_and_replies_and_nothing_else() {
        let l = log(&[
            user("fix the bug"),
            assistant(
                "m1",
                json!({"type":"thinking","thinking":"SECRET REASONING"}),
                50,
            ),
            assistant(
                "m1",
                json!({"type":"tool_use","name":"Read","input":{"file_path":"/secret/path"}}),
                50,
            ),
            json!({"type":"user","timestamp":"T","toolUseResult":"SECRET FILE BODY","message":{"content":[
                {"type":"tool_result","content":"SECRET FILE BODY"}]}}),
            json!({"type":"attachment","content":"SECRET ATTACHMENT"}),
            json!({"type":"api-request-blob","blob":"SECRET BLOB"}),
            assistant("m2", json!({"type":"text","text":"Fixed it."}), 7),
        ]);
        let d = digest_lines(&l, 0, usize::MAX);
        assert_eq!(d.turns.len(), 2);
        assert_eq!(d.turns[0].role, Role::User);
        assert_eq!(d.turns[0].text, "fix the bug");
        assert_eq!(d.turns[1].text, "Fixed it.");

        let rendered = render(
            &Source {
                step: "001-x",
                session: "s",
                first_line: 0,
                line_count: 7,
                log_sha256: "abc",
                commits: &["deadbeef".to_string()],
            },
            &d,
        );
        assert!(!rendered.contains("SECRET"), "{rendered}");
        assert!(!rendered.contains("/secret/path"));
        assert!(rendered.contains("fix the bug"));
        assert!(rendered.contains("- commits: deadbeef"));
        assert!(rendered.contains("- log lines: 1–7 (7 lines)"));
    }

    #[test]
    fn tokens_count_each_api_response_once() {
        let l = log(&[
            assistant("m1", json!({"type":"thinking","thinking":"x"}), 50),
            assistant("m1", json!({"type":"text","text":"a"}), 50),
            assistant("m2", json!({"type":"text","text":"b"}), 7),
        ]);
        let d = digest_lines(&l, 0, usize::MAX);
        assert_eq!(
            d.tokens,
            Tokens {
                input: 4,
                output: 57,
                cache_read: 200,
                cache_write: 20
            }
        );
    }

    #[test]
    fn injected_blocks_meta_and_sidechain_records_are_dropped() {
        let l = log(&[
            user("<system-reminder>\nCONTENTS OF A PRIVATE FILE\n</system-reminder>\nreal prompt"),
            user("<local-command-stdout>noise</local-command-stdout>"),
            json!({"type":"user","isMeta":true,"message":{"content":"meta noise"}}),
            json!({"type":"user","isSidechain":true,"message":{"content":"subagent prompt"}}),
            user("<system-reminder>unterminated PRIVATE"),
        ]);
        let d = digest_lines(&l, 0, usize::MAX);
        assert_eq!(d.turns.len(), 1);
        assert_eq!(d.turns[0].text, "real prompt");
    }

    #[test]
    fn only_the_requested_line_range_is_digested() {
        let l = log(&[
            user("step one prompt"),
            assistant("m1", json!({"type":"text","text":"one done"}), 5),
            user("step two prompt"),
            assistant("m2", json!({"type":"text","text":"two done"}), 9),
        ]);
        let d = digest_lines(&l, 2, 2);
        assert_eq!(d.turns.len(), 2);
        assert_eq!(d.turns[0].text, "step two prompt");
        assert_eq!(d.tokens.output, 9);
    }

    #[test]
    fn consecutive_records_of_one_speaker_merge() {
        let l = log(&[
            assistant("m1", json!({"type":"text","text":"first"}), 1),
            assistant("m2", json!({"type":"text","text":"second"}), 1),
        ]);
        let d = digest_lines(&l, 0, usize::MAX);
        assert_eq!(d.turns.len(), 1);
        assert_eq!(d.turns[0].text, "first\n\nsecond");
    }

    #[test]
    fn sha256_matches_a_known_vector() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("f");
        std::fs::write(&p, "abc").unwrap();
        assert_eq!(
            sha256_file(&p).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn write_skips_when_nothing_new_or_no_conversation() {
        let tmp = tempfile::tempdir().unwrap();
        let logp = tmp.path().join("s.jsonl");
        std::fs::write(&logp, log(&[json!({"type":"mode","mode":"x"})])).unwrap();
        let step_dir = tmp.path().join("001-x");
        std::fs::create_dir_all(&step_dir).unwrap();

        let none_new = crate::session::Snapshot {
            path: logp.clone(),
            first_new_line: 1,
            new_lines: 0,
        };
        assert!(
            write_step_transcript(&step_dir, "001-x", &none_new, &[])
                .unwrap()
                .is_none()
        );
        let no_talk = crate::session::Snapshot {
            path: logp.clone(),
            first_new_line: 0,
            new_lines: 1,
        };
        assert!(
            write_step_transcript(&step_dir, "001-x", &no_talk, &[])
                .unwrap()
                .is_none()
        );
        assert!(!step_dir.join(TRANSCRIPT_FILE).exists());

        std::fs::write(&logp, log(&[user("hello")])).unwrap();
        let path = write_step_transcript(&step_dir, "001-x", &no_talk, &[])
            .unwrap()
            .unwrap();
        let body = std::fs::read_to_string(path).unwrap();
        assert!(body.contains("# Transcript: step 001-x"));
        assert!(body.contains("- session: `s`"));
        assert!(body.contains("hello"));
    }
}
