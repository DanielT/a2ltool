#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    a2ltool::fuzz::fuzz_dwarf(data);
});
