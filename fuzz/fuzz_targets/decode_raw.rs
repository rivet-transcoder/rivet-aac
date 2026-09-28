//! The first byte sizes an AudioSpecificConfig, the rest are access units
//! split on the following bytes: an error or frames, never a panic.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((&n, rest)) = data.split_first() else { return };
    let n = usize::from(n % 8).min(rest.len());
    let (asc, mut rest) = rest.split_at(n);
    let Ok(mut dec) = aac::decode::Decoder::new_raw(asc) else { return };
    while let Some((&len, tail)) = rest.split_first() {
        let len = usize::from(len).min(tail.len());
        let _ = dec.decode(&tail[..len]);
        rest = &tail[len..];
    }
});
