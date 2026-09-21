# Fuzzing a2ltool

Fuzz targets run under [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) (libFuzzer) and require a nightly toolchain. Seeds and crash artifacts in this directory are also replayed by `cargo test` on stable, so findings stay regression-tested without running a fuzzer.

## Layout

| path | checked in | purpose |
|---|---|---|
| `fuzz_targets/*.rs` | yes | one-line wrappers around `a2ltool::fuzz::*` |
| `dictionaries/*.dict` | yes | A2L and `@@`-directive keyword dictionaries |
| `seeds/<target>/` | yes | hand-curated starting inputs |
| `artifacts/<target>/` | yes | crash reproducers, replayed as regression tests |
| `artifacts/<target>/known_*/` | yes | reproducers for bugs not yet fixed |
| `corpus/<target>/` | no (gitignored) | libFuzzer's working corpus |

Harness logic lives in `../src/fuzz.rs` so the libFuzzer targets and the replay tests execute identical code.

## Running

```sh
cargo install cargo-fuzz

mkdir -p fuzz/corpus/fuzz_dwarf
cargo +nightly fuzz run fuzz_dwarf fuzz/corpus/fuzz_dwarf fuzz/seeds/fuzz_dwarf -- \
    -max_len=65536 -rss_limit_mb=2048 -malloc_limit_mb=256 -max_total_time=600
```

Recommended `-max_len` and dictionary per target:

| target | `-max_len` | `-dict=` |
|---|---|---|
| `fuzz_a2l_parse`, `fuzz_a2l_roundtrip`, `fuzz_a2l_pipeline` | 16384 | `fuzz/dictionaries/a2l.dict` |
| `fuzz_creator_scan`, `fuzz_creator_source` | 8192 | `fuzz/dictionaries/creator.dict` |
| `fuzz_dwarf`, `fuzz_pdb` | 65536 | - |
| `fuzz_symbol_lookup` | 256 | - |

Always pass `-rss_limit_mb` and `-malloc_limit_mb`: `pdb2` allocates based on a length read directly from the input, so a corrupt PDB triggers a huge allocation before the read that would have failed. Without these limits, such cases appear as unhelpful OOM kills.

`-close_fd_mask=3` silences the per-type `println!`s in the PDB reader. It helps throughput but hides panic messages, so leave it off while triaging.

## Bootstrapping a bigger corpus

The large fixtures are too big to check in as seeds but make excellent corpus material locally:

```sh
cargo +nightly fuzz run fuzz_dwarf fuzz/corpus/fuzz_dwarf fixtures/bin -- -runs=0
cargo +nightly fuzz cmin fuzz_dwarf
```

Promote anything small that survives `cmin` into `seeds/`.

## Triage

```sh
cargo +nightly fuzz run   <target> <artifact>              # reproduce
cargo +nightly fuzz tmin  <target> <artifact>              # minimize
cargo test -- --nocapture <target>                         # replay on stable
```

`tmin` writes intermediate steps into `artifacts/<target>/` as `minimized-from-*`. Remove them once you have the final minimized input; they all crash and will break the replay test.

Stack overflows abort the process and cannot be caught by `catch_unwind`. The test prints each filename before running it; use `cargo test -- --nocapture` to identify the culprit.

## Adding a target

1. Add `pub fn fuzz_<name>(data: &[u8])` to `../src/fuzz.rs`.
2. Add it to the `TARGETS` table in the same file and give it a `#[test] fn replay_<name>`.
3. Create `fuzz_targets/fuzz_<name>.rs` delegating to it, and a `[[bin]]` in `Cargo.toml`.
4. Add seeds under `seeds/fuzz_<name>/`.

The `all_fuzz_targets_are_registered` test fails if step 2 is skipped.

`[[bin]]` names are snake_case, matching both the `a2ltool::fuzz::*` functions and cargo-fuzz's convention. The kebab-case warning is suppressed via `non_kebab_case_bins = "allow"` in `fuzz/Cargo.toml`.

The `[[bin]]` name also becomes the `-artifact_prefix` directory, so it must match the `seeds/`, `artifacts/`, and `corpus/` directory names and the `TARGETS` table. Renaming a target means renaming all four.

## Known limitations

- Inputs containing `/include` are rejected: a2lfile's tokenizer resolves includes from disk, which would make findings filesystem-dependent and expose arbitrary path access to the fuzzer.
