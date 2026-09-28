//! Any bytes, as an ADTS stream in two chunks: an error or frames, never a
//! panic.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let split = data.first().map_or(0, |&b| usize::from(b)).min(data.len());
    let mut dec = aac::decode::Decoder::new_adts();
    let _ = dec.decode(&data[..split]);
    let _ = dec.decode(&data[split..]);
});
