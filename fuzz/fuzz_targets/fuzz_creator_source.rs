#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    a2ltool::fuzz::fuzz_creator_source(data);
});
