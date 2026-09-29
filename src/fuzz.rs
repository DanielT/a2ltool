//! Fuzzing harness bodies, shared between the libFuzzer targets in `fuzz/` and the
//! corpus-replay tests at the bottom of this file.
//!
//! Every `fuzz_*` function must be panic-free for any input, except where a
//! deliberate oracle assertion is documented. Errors returned by the code under test
//! are expected results and are simply discarded.
//!
//! This module is `pub` so that the `fuzz/` crate can link against it. It is not a
//! stable API and carries no semver guarantees.

use crate::debuginfo::DebugData;
use crate::{A2lVersion, creator, ifdata, remove, symbol, version};

// Size caps are enforced here in addition to libFuzzer's `-max_len`, so that a corpus
// file copied in by hand cannot blow up the replay test's runtime. They are set
// deliberately larger than the recommended `-max_len` so they never silently mask an
// input that libFuzzer itself would have produced.
const MAX_A2L_LEN: usize = 64 * 1024;
const MAX_SRC_LEN: usize = 64 * 1024;
const MAX_BIN_LEN: usize = 4 * 1024 * 1024;
const MAX_NAME_LEN: usize = 1024;

/// ORACLE: `write_to_string()` must be a fixpoint after one round.
/// If this produces noisy findings on real files, set it to `false`. The roundtrip
/// target will still check that re-parsing never fails.
const ORACLE_ROUNDTRIP_FIXPOINT: bool = true;

/// A minimal A2L file that always has exactly one MODULE, used as the donor in the
/// merge step of `fuzz_a2l_pipeline`.
const MERGE_DONOR: &str = r#"ASAP2_VERSION 1 71
/begin PROJECT donor ""
  /begin MODULE donor_mod ""
  /end MODULE
/end PROJECT
"#;

fn a2ml_spec() -> Option<String> {
    Some(ifdata::A2MLVECTOR_TEXT.to_string())
}

/// Convert fuzzer bytes into A2L source text, or `None` if the input must be skipped.
///
/// Uses `from_utf8_lossy` rather than rejecting non-UTF-8: byte-level mutations break
/// UTF-8 constantly, and rejecting them would discard most of the fuzzer's work. Lossy
/// conversion is deterministic, so replay reproduces exactly.
///
/// `/include` is rejected because a2lfile's tokenizer resolves include directives by
/// reading files from disk. Without this guard, the fuzzer would issue filesystem I/O
/// at attacker-controlled paths, making findings nondeterministic and unreproducible.
fn a2l_text(data: &[u8]) -> Option<String> {
    if data.len() > MAX_A2L_LEN {
        return None;
    }
    if memchr::memmem::find(data, b"/include").is_some() {
        return None;
    }
    Some(String::from_utf8_lossy(data).into_owned())
}

/// Target: load A2L text, mirroring the primary input path of `core()` including the
/// module-fragment fallback.
pub fn fuzz_a2l_parse(data: &[u8]) {
    let Some(text) = a2l_text(data) else { return };
    match a2lfile::load_from_string(&text, a2ml_spec(), false) {
        Ok((a2l, _log_msgs)) => {
            let _ = a2l.write_to_string();
        }
        Err(_) => {
            let _ = a2lfile::load_fragment(&text, a2ml_spec());
        }
    }
}

/// Target: parse -> write -> parse -> write differential.
pub fn fuzz_a2l_roundtrip(data: &[u8]) {
    let Some(text) = a2l_text(data) else { return };
    a2l_roundtrip_check(&text);
}

/// The roundtrip oracle, without a size cap. Also used by the `roundtrip_fixtures` test
/// to validate the oracle against real-world files.
///
/// # Panics
/// Panics if a written A2L file cannot be re-parsed, or (when
/// `ORACLE_ROUNDTRIP_FIXPOINT` is enabled) if writing is not a fixpoint.
pub fn a2l_roundtrip_check(text: &str) {
    let Ok((f1, _)) = a2lfile::load_from_string(text, a2ml_spec(), false) else {
        return;
    };
    let t1 = f1.write_to_string();
    let (f2, _) = match a2lfile::load_from_string(&t1, a2ml_spec(), false) {
        Ok(v) => v,
        Err(e) => panic!("ORACLE roundtrip: output of write_to_string failed to re-parse: {e}"),
    };
    let t2 = f2.write_to_string();
    if ORACLE_ROUNDTRIP_FIXPOINT {
        assert!(
            t1 == t2,
            "ORACLE roundtrip: write(parse(x)) is not a fixpoint"
        );
    }
}

/// Target: the a2ltool operations that need no debug info - check, version conversion,
/// remove, sort, cleanup, and merge.
pub fn fuzz_a2l_pipeline(data: &[u8]) {
    let Some(text) = a2l_text(data) else { return };
    let Ok((mut a2l, _)) = a2lfile::load_from_string(&text, a2ml_spec(), false) else {
        return;
    };

    let _ = a2l.check();

    // Every version conversion, each on a fresh clone so they cannot mask one another.
    for v in [
        A2lVersion::V1_5_0,
        A2lVersion::V1_5_1,
        A2lVersion::V1_6_0,
        A2lVersion::V1_6_1,
        A2lVersion::V1_7_0,
        A2lVersion::V1_7_1,
    ] {
        let mut converted = a2l.clone();
        version::convert(&mut converted, v);
        let _ = converted.write_to_string();
    }

    // Fixed patterns: compiling fuzzer-controlled regexes is not what this target tests.
    let _ = remove::remove_items(&mut a2l, &["a.*", "^x$", ".*"]);
    let _ = remove::remove_address_ranges(&mut a2l, &[(0, 0x1000), (0x8000, u64::MAX)]);

    a2l.sort();
    a2l.cleanup();
    a2l.ifdata_cleanup();

    // Mirrors the `--merge` path in lib.rs. lib.rs rejects a PROJECT with no MODULE
    // before reaching this point, so the harness applies the same guard. Without it,
    // the fuzzer would report an internal indexing error in a2lfile that a2ltool
    // can no longer trigger.
    if !a2l.project.module.is_empty()
        && let Ok((mut donor, _)) = a2lfile::load_from_string(MERGE_DONOR, a2ml_spec(), false)
        && !donor.project.module.is_empty()
    {
        let donor_module = &mut donor.project.module[0];
        a2l.project.module[0].merge(donor_module);
    }

    // ORACLE: whatever the pipeline produced must still be parseable.
    let out = a2l.write_to_string();
    if let Err(e) = a2lfile::load_from_string(&out, a2ml_spec(), false) {
        panic!(
            "ORACLE pipeline: output after convert/remove/sort/cleanup/merge is unparseable: {e}"
        );
    }
}

/// Target: the `@@` comment scanner alone. Cheap, so it reaches high exec/s.
pub fn fuzz_creator_scan(data: &[u8]) {
    if data.len() > MAX_SRC_LEN {
        return;
    }
    let _ = creator::scan_comments_only(data);
}

/// Target: scanner -> definition parser -> A2L item construction.
///
/// This deliberately goes through the scanner rather than calling
/// `parser::parse_definitions` directly. That function panics on an empty token, but
/// the scanner never produces one, so a direct target would report a false positive.
pub fn fuzz_creator_source(data: &[u8]) {
    if data.len() > MAX_SRC_LEN {
        return;
    }
    let mut a2l_flat = a2lfile::new();
    let flat_ok = creator::create_items_from_data(&mut a2l_flat, data, None, false, false).is_ok();

    let mut a2l_struct = a2lfile::new();
    let struct_ok = creator::create_items_from_data(
        &mut a2l_struct,
        data,
        Some("fuzz_group".to_string()),
        true,
        false,
    )
    .is_ok();

    // ORACLE: anything the creator builds successfully must be writable and
    // re-parseable. The success filter is essential: on error, `core()` bails out
    // with "Exiting because errors were found during item creation" and never
    // writes the file. A partially-built A2L is discarded in that case, so
    // asserting on it would be unfair.
    for (a2l, _) in [(&a2l_flat, flat_ok), (&a2l_struct, struct_ok)]
        .into_iter()
        .filter(|(_, ok)| *ok)
    {
        let out = a2l.write_to_string();
        if let Err(e) = a2lfile::load_from_string(&out, None, false) {
            panic!("ORACLE creator: generated A2L is unparseable: {e}");
        }
    }
}

/// Target: the ELF/DWARF reader, fed raw bytes.
pub fn fuzz_dwarf(data: &[u8]) {
    if data.len() > MAX_BIN_LEN {
        return;
    }
    let _ = DebugData::load_dwarf_from_slice("<fuzz>", data, false);
}

/// Target: the PDB reader, fed raw bytes.
pub fn fuzz_pdb(data: &[u8]) {
    if data.len() > MAX_BIN_LEN {
        return;
    }
    let _ = DebugData::load_pdb_from_slice("<fuzz>", data, false);
}

/// A small ELF containing arrays, nested arrays, and a struct. It is embedded so that
/// the symbol target has no runtime file dependency.
static FUZZ_ELF: &[u8] = include_bytes!("../fixtures/bin/nested_array_test.elf");

/// Returns `None` if the embedded fixture is an unresolved Git-LFS pointer, so the
/// target degrades to a no-op instead of failing.
fn fuzz_debugdata() -> Option<&'static DebugData> {
    static DBG: std::sync::OnceLock<Option<DebugData>> = std::sync::OnceLock::new();
    DBG.get_or_init(|| DebugData::load_dwarf_from_slice("<fuzz>", FUZZ_ELF, false).ok())
        .as_ref()
}

/// Target: symbol-name resolution, including the Vector extended syntax, against a
/// fixed set of debug data.
pub fn fuzz_symbol_lookup(data: &[u8]) {
    if data.len() > MAX_NAME_LEN {
        return;
    }
    let Some(dbg) = fuzz_debugdata() else { return };
    let name = String::from_utf8_lossy(data);
    if let Ok(sym) = symbol::find_symbol(&name, dbg) {
        for offset in [0i32, 4, 12, -1, i32::MAX] {
            let _ = symbol::find_symbol_by_offset(&sym, offset, dbg, true);
            let _ = symbol::find_symbol_by_offset(&sym, offset, dbg, false);
        }
    }
}

#[cfg(test)]
mod replay {
    use std::{
        fs,
        panic::{self, AssertUnwindSafe},
        path::{Path, PathBuf},
    };

    type Harness = fn(&[u8]);

    /// Maps the directory name under `fuzz/{seeds,artifacts,corpus}` to its harness.
    /// Every file in `fuzz/fuzz_targets/` must appear here - see
    /// `all_fuzz_targets_are_registered`.
    const TARGETS: &[(&str, Harness)] = &[
        ("fuzz_a2l_parse", super::fuzz_a2l_parse as Harness),
        ("fuzz_a2l_roundtrip", super::fuzz_a2l_roundtrip as Harness),
        ("fuzz_a2l_pipeline", super::fuzz_a2l_pipeline as Harness),
        ("fuzz_creator_scan", super::fuzz_creator_scan as Harness),
        ("fuzz_creator_source", super::fuzz_creator_source as Harness),
        ("fuzz_dwarf", super::fuzz_dwarf as Harness),
        ("fuzz_pdb", super::fuzz_pdb as Harness),
        ("fuzz_symbol_lookup", super::fuzz_symbol_lookup as Harness),
    ];

    fn fuzz_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("fuzz")
    }

    fn inputs_for(target: &str) -> Vec<PathBuf> {
        let base = fuzz_dir();
        let mut files = Vec::new();
        // seeds/ and artifacts/ are checked in. corpus/ is gitignored but replayed
        // when present, so findings from a local fuzzing session are re-checked by
        // `cargo test`.
        for sub in ["seeds", "artifacts", "corpus"] {
            let dir = base.join(sub).join(target);
            // A missing directory is a skip, not a failure.
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with('.') || name.ends_with(".md") {
                    continue;
                }
                files.push(path);
            }
        }
        files.sort();
        files
    }

    /// Replay all inputs of one target on a thread with a known stack size.
    ///
    /// libtest gives each test thread a smaller stack than the main thread that a2ltool
    /// itself runs on, so without this the replay would report stack exhaustion for inputs
    /// that the real program handles. 8 MB matches the usual main thread stack.
    fn replay(target: &str, harness: Harness) {
        let target = target.to_string();
        let handle = std::thread::Builder::new()
            .stack_size(8 * 1024 * 1024)
            .name(format!("replay_{target}"))
            .spawn(move || replay_on_current_thread(&target, harness))
            .expect("failed to spawn the replay thread");
        if let Err(payload) = handle.join() {
            // propagate an assertion failure from the replay thread, so that the test fails
            panic::resume_unwind(payload);
        }
    }

    fn replay_on_current_thread(target: &str, harness: Harness) {
        let mut failures: Vec<String> = Vec::new();
        let mut count = 0usize;
        for path in inputs_for(target) {
            let Ok(data) = fs::read(&path) else { continue };
            // Printed before the run: a hard abort (stack overflow, OOM) cannot be
            // caught, but `cargo test -- --nocapture` will then show the offending file.
            eprintln!("[fuzz-replay] {target}: {}", path.display());
            let prev = panic::take_hook();
            let panic_path = path.clone();
            panic::set_hook(Box::new(move |info| {
                eprintln!("[fuzz-replay] PANIC in {}: {info}", panic_path.display());
            }));
            let result = panic::catch_unwind(AssertUnwindSafe(|| harness(&data)));
            panic::set_hook(prev);
            if result.is_err() {
                failures.push(path.display().to_string());
            }
            count += 1;
        }
        eprintln!("[fuzz-replay] {target}: {count} inputs replayed");
        assert!(
            failures.is_empty(),
            "{target}: {} of {count} corpus inputs panicked:\n  {}",
            failures.len(),
            failures.join("\n  ")
        );
    }

    fn replay_named(target: &str) {
        let harness = TARGETS
            .iter()
            .find(|(name, _)| *name == target)
            .map(|(_, h)| *h)
            .unwrap_or_else(|| panic!("no harness registered for {target}"));
        replay(target, harness);
    }

    #[test]
    fn replay_a2l_parse() {
        replay_named("fuzz_a2l_parse");
    }

    #[test]
    fn replay_a2l_roundtrip() {
        replay_named("fuzz_a2l_roundtrip");
    }

    #[test]
    fn replay_a2l_pipeline() {
        replay_named("fuzz_a2l_pipeline");
    }

    #[test]
    fn replay_creator_scan() {
        replay_named("fuzz_creator_scan");
    }

    #[test]
    fn replay_creator_source() {
        replay_named("fuzz_creator_source");
    }

    #[test]
    fn replay_dwarf() {
        replay_named("fuzz_dwarf");
    }

    #[test]
    fn replay_pdb() {
        replay_named("fuzz_pdb");
    }

    #[test]
    fn replay_symbol_lookup() {
        replay_named("fuzz_symbol_lookup");
    }

    /// Guards against a newly added fuzz target being silently left out of replay.
    #[test]
    fn all_fuzz_targets_are_registered() {
        let dir = fuzz_dir().join("fuzz_targets");
        let Ok(entries) = fs::read_dir(&dir) else {
            return; // fuzz/ not present (e.g. a source-only checkout)
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let stem = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            assert!(
                TARGETS.iter().any(|(name, _)| *name == stem),
                "fuzz target '{stem}' has no entry in the TARGETS table in src/fuzz.rs"
            );
        }
    }

    /// Validates the roundtrip oracle against every real fixture. Run this before
    /// trusting `ORACLE_ROUNDTRIP_FIXPOINT` on fuzzer-generated input.
    #[test]
    fn roundtrip_fixtures() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/a2l");
        let Ok(entries) = fs::read_dir(&dir) else {
            return;
        };
        let mut paths: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("a2l"))
            // merge_inc_test.a2l uses /include, which the harness rejects.
            .filter(|p| p.file_name().is_some_and(|n| n != "merge_inc_test.a2l"))
            .collect();
        paths.sort();
        for path in paths {
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            eprintln!("[roundtrip] {}", path.display());
            super::a2l_roundtrip_check(&text);
        }
    }
}
