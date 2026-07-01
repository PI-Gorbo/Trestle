//! Corpus harness: parse every `.trsl` program under `programs/`.
//!
//! Contract: a program is **expected to parse** unless it opts out with a
//! `// @skip:` directive on a comment line. As the language grows, delete the
//! `@skip` line from a file and it joins the must-parse set — so the shrinking
//! skip list is the remaining roadmap.
//!
//! Run `cargo test -p trestle-tests -- --nocapture` to see the parsed/skipped
//! checklist even when the suite is green (Cargo hides output for passing tests).

use std::fs;
use std::path::{Path, PathBuf};

/// Recursively collect `*.trsl` files under `dir`, sorted for deterministic order.
fn collect_trsl(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .map(|e| e.expect("dir entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_trsl(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "trsl") {
            out.push(path);
        }
    }
}

/// If the source opts out via a `// @skip[: reason]` comment, return the reason.
fn skip_reason(src: &str) -> Option<String> {
    src.lines().find_map(|line| {
        let comment = line.trim_start().strip_prefix("//")?.trim_start();
        let rest = comment.strip_prefix("@skip")?;
        let reason = rest.trim_start_matches(':').trim();
        Some(if reason.is_empty() {
            "(no reason given)".to_string()
        } else {
            reason.to_string()
        })
    })
}

#[test]
fn corpus_parses() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("programs");
    let mut files = Vec::new();
    collect_trsl(&root, &mut files);
    assert!(
        !files.is_empty(),
        "no .trsl files found under {}",
        root.display()
    );

    let mut parsed = 0usize;
    let mut skipped: Vec<(PathBuf, String)> = Vec::new();
    let mut failures: Vec<(PathBuf, String)> = Vec::new();

    for path in &files {
        let src = fs::read_to_string(path).expect("read .trsl file");
        let rel = path.strip_prefix(&root).unwrap_or(path).to_path_buf();
        match skip_reason(&src) {
            Some(reason) => skipped.push((rel, reason)),
            None => match trestle::parse(&src) {
                Ok(_) => parsed += 1,
                Err(e) => failures.push((rel, e.to_string())),
            },
        }
    }

    // Progress checklist (visible with `-- --nocapture`, or always on failure).
    eprintln!(
        "\n=== Trestle corpus: {parsed} parsed / {} skipped / {} failed ===",
        skipped.len(),
        failures.len()
    );
    if !skipped.is_empty() {
        eprintln!("\nskipped — delete the `// @skip:` line once the feature lands:");
        for (path, reason) in &skipped {
            eprintln!("  · {}  — {reason}", path.display());
        }
    }

    if !failures.is_empty() {
        let mut msg =
            String::from("\nfiles expected to parse but did not (remove @skip only when ready):\n");
        for (path, err) in &failures {
            msg.push_str(&format!("\n----- {} -----\n{err}\n", path.display()));
        }
        panic!("{msg}");
    }
}

/// Snapshot the parsed AST of every corpus program that is expected to parse.
///
/// This pins down the parse tree: a grammar or walker change that silently
/// alters the AST for any file fails here with a diff. `corpus_parses` remains
/// the authority on whether a file parses at all; this test only snapshots the
/// ones that do (and aren't `@skip`'d).
///
/// First run writes `*.snap.new` files and fails; promote them with
/// `cargo insta accept` (or review with `cargo insta review`).
#[test]
fn corpus_ast_snapshots() {
    insta::glob!("..", "programs/**/*.trsl", |path| {
        let src = fs::read_to_string(path).expect("read .trsl file");

        // Files opting out with `// @skip:` aren't guaranteed to parse yet — skip.
        if skip_reason(&src).is_some() {
            return;
        }

        // Parse failures are reported by `corpus_parses`; don't double-panic here.
        let Ok(program) = trestle::parse(&src) else {
            return;
        };

        insta::assert_debug_snapshot!(program);
    });
}
