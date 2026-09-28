//! Decoder tests that need no external tools: round trips through this
//! crate's encoder, the transports, and malformed input. The comparisons
//! against ffmpeg's decoder are in `tests/ffmpeg_oracle.rs`.

use super::*;
use crate::encode::{self, Encoder, EncoderConfig};

fn sine(freq: f64, amp: f64, rate: u32, len: usize) -> Vec<f32> {
    (0..len)
        .map(|i| (amp * (2.0 * std::f64::consts::PI * freq * i as f64 / f64::from(rate)).sin()) as f32)
        .collect()
}

fn interleave(chans: &[Vec<f32>]) -> Vec<f32> {
    (0..chans[0].len())
        .flat_map(|i| chans.iter().map(move |c| c[i]))
        .collect()
}

fn encode(chans: &[Vec<f32>], rate: u32) -> (Encoder, Vec<Vec<u8>>) {
    let mut enc = Encoder::new(EncoderConfig {
        sample_rate: rate,
        channels: chans.len() as u8,
        bitrate: 0,
    })
    .unwrap();
    let mut aus = enc.encode(&interleave(chans));
    aus.extend(enc.flush());
    (enc, aus)
}

fn snr_db(reference: &[f32], decoded: &[f32]) -> f64 {
    let (mut s, mut n) = (0.0f64, 0.0f64);
    for (&a, &b) in reference.iter().zip(decoded) {
        s += f64::from(a) * f64::from(a);
        n += f64::from(a - b) * f64::from(a - b);
    }
    10.0 * (s / n.max(1e-30)).log10()
}

/// Encode one tone per channel with this crate's encoder, decode it with
/// this decoder, and find every tone in its own slot.
#[test]
fn round_trips_every_layout_through_the_encoder() {
    for (channels, speakers) in [
        (1u8, vec![Speaker::FC]),
        (2, vec![Speaker::FL, Speaker::FR]),
        (3, vec![Speaker::FL, Speaker::FR, Speaker::FC]),
        (4, vec![Speaker::FL, Speaker::FR, Speaker::FC, Speaker::BC]),
        (5, vec![Speaker::FL, Speaker::FR, Speaker::FC, Speaker::BL, Speaker::BR]),
        (6, vec![Speaker::FL, Speaker::FR, Speaker::FC, Speaker::LFE, Speaker::BL, Speaker::BR]),
        (
            8,
            vec![
                Speaker::FL,
                Speaker::FR,
                Speaker::FC,
                Speaker::LFE,
                Speaker::BL,
                Speaker::BR,
                Speaker::SL,
                Speaker::SR,
            ],
        ),
    ] {
        let rate = 48_000;
        let len = rate as usize;
        let chans: Vec<Vec<f32>> = (0..channels)
            .map(|c| {
                // The LFE (slot 3 of 5.1 and 7.1) carries only the lowest lines.
                let f = if channels >= 6 && c == 3 { 60.0 } else { 300.0 + 150.0 * f64::from(c) };
                sine(f, 0.3, rate, len)
            })
            .collect();
        let (enc, aus) = encode(&chans, rate);
        let mut dec = Decoder::new_raw(&enc.audio_specific_config()).unwrap();
        assert_eq!(dec.speakers().unwrap(), speakers.as_slice());
        let mut out: Vec<Vec<f32>> = vec![Vec::new(); usize::from(channels)];
        for au in &aus {
            let f = dec.decode(au).unwrap().remove(0);
            assert_eq!(f.speakers.as_ref(), Some(&speakers));
            for (i, &v) in f.samples.iter().enumerate() {
                out[i % usize::from(channels)].push(v);
            }
        }
        for (c, o) in out.iter_mut().enumerate() {
            o.drain(..encode::ENCODER_DELAY as usize);
            let snr = snr_db(&chans[c][2048..len - 1024], &o[2048..len - 1024]);
            assert!(snr > 30.0, "{channels} ch, channel {c}: {snr:.1} dB");
        }
    }
}

#[test]
fn adts_in_any_chunking_decodes_the_same_as_raw() {
    let rate = 44_100;
    let chans = vec![sine(440.0, 0.4, rate, 20_000), sine(660.0, 0.3, rate, 20_000)];
    let (enc, aus) = encode(&chans, rate);
    let adts: Vec<u8> = aus
        .iter()
        .flat_map(|au| encode::adts_frame(enc.sampling_index(), enc.channel_configuration(), au))
        .collect();
    let mut raw = Decoder::new_raw(&enc.audio_specific_config()).unwrap();
    let want: Vec<f32> = aus.iter().flat_map(|au| raw.decode(au).unwrap().remove(0).samples).collect();
    for chunk in [1usize, 7, 100, 1000, adts.len()] {
        let mut dec = Decoder::new_adts();
        // Garbage before the first frame is skipped to the syncword.
        let mut got: Vec<f32> = dec.decode(&[0x00, 0xff, 0x12, 0x34]).unwrap().into_iter().flat_map(|f| f.samples).collect();
        for c in adts.chunks(chunk) {
            for f in dec.decode(c).unwrap() {
                assert_eq!(f.sample_rate, rate);
                got.extend(f.samples);
            }
        }
        assert_eq!(got, want, "chunk {chunk}");
    }
}

#[test]
fn silence_decodes_to_silence() {
    let (enc, aus) = encode(&[vec![0.0; 5000]], 32_000);
    let mut dec = Decoder::new_raw(&enc.audio_specific_config()).unwrap();
    for au in &aus {
        assert!(dec.decode(au).unwrap()[0].samples.iter().all(|&v| v == 0.0));
    }
}

#[test]
fn malformed_input_is_an_error_not_a_panic() {
    let (enc, aus) = encode(&[sine(1000.0, 0.5, 48_000, 10_000)], 48_000);
    let asc = enc.audio_specific_config();
    let mut seed = 1u32;
    let mut rnd = || {
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        seed >> 8
    };
    for au in &aus {
        for _ in 0..200 {
            let mut bad = au.clone();
            match rnd() % 3 {
                0 => bad.truncate(rnd() as usize % (au.len() + 1)),
                1 => {
                    for _ in 0..1 + rnd() % 4 {
                        let i = rnd() as usize % bad.len().max(1);
                        if let Some(b) = bad.get_mut(i) {
                            *b ^= 1 << (rnd() % 8);
                        }
                    }
                }
                _ => bad = (0..rnd() % 64).map(|_| rnd() as u8).collect(),
            }
            let mut dec = Decoder::new_raw(&asc).unwrap();
            let _ = dec.decode(&bad);
            let mut adts = Decoder::new_adts();
            let _ = adts.decode(&bad);
        }
    }
}

/// A fill element whose escaped count is 0 carries 14 bytes (cnt 15 plus
/// esc_count 0 minus 1): no arithmetic overflow on the way.
#[test]
fn a_fill_element_with_a_zero_escape_count_is_skipped() {
    // FIL (110) cnt 15 (1111) esc_count 0, 14 zero bytes, END (111).
    let mut bits = String::from("110") + "1111" + "00000000";
    bits += &"0".repeat(14 * 8);
    bits += "111";
    while !bits.len().is_multiple_of(8) {
        bits.push('0');
    }
    let au: Vec<u8> = (0..bits.len() / 8).map(|i| u8::from_str_radix(&bits[8 * i..8 * i + 8], 2).unwrap()).collect();
    let mut dec = Decoder::new_raw(&[0x12, 0x08]).unwrap(); // LC 44.1 kHz mono
    let f = dec.decode(&au).unwrap().remove(0);
    assert!(f.samples.iter().all(|&v| v == 0.0));
}
