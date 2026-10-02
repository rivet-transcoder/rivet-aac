//! The complex-exponential QMF banks of the SBR tool (ISO/IEC 14496-3
//! subclause 4.6.18.4, Figures 4.42 to 4.44) and the encoder's 64-band
//! analysis bank (informative subclause 4.B.18.2, Figure 4.B.16), computed
//! as the flowcharts state them: window, fold, then the modulation matrix.
//! The window is Table 4.A.89.

use std::f64::consts::PI;
use std::sync::OnceLock;

use super::Cplx;
use crate::tables::sbr::QMF_WINDOW;

/// The window `c` as `f32`.
fn window() -> &'static [f32; 640] {
    static W: OnceLock<[f32; 640]> = OnceLock::new();
    W.get_or_init(|| QMF_WINDOW.map(|c| c as f32))
}

/// A modulation matrix stored as `(cos, sin)` rows: `rows` outputs of
/// `cols` inputs each, `phase(row, col)` radians.
struct Matrix {
    cols: usize,
    re: Vec<f32>,
    im: Vec<f32>,
}

impl Matrix {
    fn new(rows: usize, cols: usize, scale: f64, phase: impl Fn(usize, usize) -> f64) -> Self {
        let mut re = Vec::with_capacity(rows * cols);
        let mut im = Vec::with_capacity(rows * cols);
        for r in 0..rows {
            for c in 0..cols {
                let p = phase(r, c);
                re.push((scale * p.cos()) as f32);
                im.push((scale * p.sin()) as f32);
            }
        }
        Self { cols, re, im }
    }

    fn row(&self, r: usize) -> (&[f32], &[f32]) {
        let at = r * self.cols;
        (&self.re[at..at + self.cols], &self.im[at..at + self.cols])
    }
}

fn analysis32_matrix() -> &'static Matrix {
    static M: OnceLock<Matrix> = OnceLock::new();
    // M(k, n) = 2 exp(i pi (k + 0.5)(2n - 0.5) / 64).
    M.get_or_init(|| Matrix::new(32, 64, 2.0, |k, n| PI / 64.0 * (k as f64 + 0.5) * (2.0 * n as f64 - 0.5)))
}

fn synthesis64_matrix() -> &'static Matrix {
    static M: OnceLock<Matrix> = OnceLock::new();
    // N(k, n) = exp(i pi (k + 0.5)(2n - 255) / 128) / 64, stored by n.
    M.get_or_init(|| {
        Matrix::new(128, 64, 1.0 / 64.0, |n, k| PI / 128.0 * (k as f64 + 0.5) * (2.0 * n as f64 - 255.0))
    })
}

fn synthesis32_matrix() -> &'static Matrix {
    static M: OnceLock<Matrix> = OnceLock::new();
    // N(k, n) = exp(i pi (k + 0.5)(2n - 127.5) / 64) / 64, stored by n.
    M.get_or_init(|| {
        Matrix::new(64, 32, 1.0 / 64.0, |n, k| PI / 64.0 * (k as f64 + 0.5) * (2.0 * n as f64 - 127.5))
    })
}

#[allow(dead_code)] // the encoder's
fn analysis64_matrix() -> &'static Matrix {
    static M: OnceLock<Matrix> = OnceLock::new();
    // M(k, n) = exp(i pi (k + 0.5)(2n + 1) / 128).
    M.get_or_init(|| Matrix::new(64, 128, 1.0, |k, n| PI / 128.0 * (k as f64 + 0.5) * (2.0 * n as f64 + 1.0)))
}

/// The decoder's 32-band analysis bank (Figure 4.42).
#[derive(Clone)]
pub(crate) struct Analysis32 {
    x: [f32; 320],
}

impl Default for Analysis32 {
    fn default() -> Self {
        Self { x: [0.0; 320] }
    }
}

impl Analysis32 {
    /// Filter 32 new input samples (oldest first) into one slot of 32
    /// subband samples.
    pub fn process(&mut self, input: &[f32], out: &mut [Cplx]) {
        debug_assert_eq!(input.len(), 32);
        let c = window();
        self.x.copy_within(0..288, 32);
        for (n, &s) in input.iter().enumerate() {
            self.x[31 - n] = s;
        }
        let mut u = [0.0f32; 64];
        for (n, un) in u.iter_mut().enumerate() {
            let mut acc = 0.0;
            for j in 0..5 {
                let i = n + 64 * j;
                acc += self.x[i] * c[2 * i];
            }
            *un = acc;
        }
        let m = analysis32_matrix();
        for (k, o) in out.iter_mut().enumerate().take(32) {
            let (re, im) = m.row(k);
            let (mut a, mut b) = (0.0f32, 0.0f32);
            for n in 0..64 {
                a += u[n] * re[n];
                b += u[n] * im[n];
            }
            *o = Cplx::new(a, b);
        }
    }
}

/// The decoder's 64-band synthesis bank (Figure 4.43).
#[derive(Clone)]
pub(crate) struct Synthesis64 {
    v: Vec<f32>,
}

impl Default for Synthesis64 {
    fn default() -> Self {
        Self { v: vec![0.0; 1280] }
    }
}

impl Synthesis64 {
    /// One slot of 64 subband samples into 64 output samples.
    pub fn process(&mut self, x: &[Cplx], out: &mut [f32]) {
        debug_assert!(x.len() >= 64 && out.len() >= 64);
        self.v.copy_within(0..1152, 128);
        let m = synthesis64_matrix();
        for n in 0..128 {
            let (re, im) = m.row(n);
            let mut acc = 0.0f32;
            for k in 0..64 {
                acc += x[k].re * re[k] - x[k].im * im[k];
            }
            self.v[n] = acc;
        }
        let c = window();
        for (k, o) in out.iter_mut().enumerate().take(64) {
            let mut acc = 0.0f32;
            for n in 0..5 {
                acc += self.v[256 * n + k] * c[128 * n + k];
                acc += self.v[256 * n + 192 + k] * c[128 * n + 64 + k];
            }
            *o = acc;
        }
    }
}

/// The decoder's downsampled 32-band synthesis bank (Figure 4.44).
#[derive(Clone)]
pub(crate) struct Synthesis32 {
    v: [f32; 640],
}

impl Default for Synthesis32 {
    fn default() -> Self {
        Self { v: [0.0; 640] }
    }
}

impl Synthesis32 {
    /// One slot of the lowest 32 subband samples into 32 output samples.
    pub fn process(&mut self, x: &[Cplx], out: &mut [f32]) {
        debug_assert!(x.len() >= 32 && out.len() >= 32);
        self.v.copy_within(0..576, 64);
        let m = synthesis32_matrix();
        for n in 0..64 {
            let (re, im) = m.row(n);
            let mut acc = 0.0f32;
            for k in 0..32 {
                acc += x[k].re * re[k] - x[k].im * im[k];
            }
            self.v[n] = acc;
        }
        let c = window();
        for (k, o) in out.iter_mut().enumerate().take(32) {
            let mut acc = 0.0f32;
            for n in 0..5 {
                acc += self.v[128 * n + k] * c[2 * (64 * n + k)];
                acc += self.v[128 * n + 96 + k] * c[2 * (64 * n + 32 + k)];
            }
            *o = acc;
        }
    }
}

/// The encoder's 64-band analysis bank (Figure 4.B.16).
#[derive(Clone)]
#[allow(dead_code)] // the encoder's
pub(crate) struct Analysis64 {
    x: Vec<f32>,
}

impl Default for Analysis64 {
    fn default() -> Self {
        Self { x: vec![0.0; 640] }
    }
}

#[allow(dead_code)] // the encoder's
impl Analysis64 {
    /// Filter 64 new input samples (oldest first) into one slot of 64
    /// subband samples.
    pub fn process(&mut self, input: &[f32], out: &mut [Cplx]) {
        debug_assert_eq!(input.len(), 64);
        let c = window();
        self.x.copy_within(0..576, 64);
        for (n, &s) in input.iter().enumerate() {
            self.x[63 - n] = s;
        }
        let mut u = [0.0f32; 128];
        for (n, un) in u.iter_mut().enumerate() {
            let mut acc = 0.0;
            for j in 0..5 {
                let i = n + 128 * j;
                acc += self.x[i] * c[i];
            }
            *un = acc;
        }
        let m = analysis64_matrix();
        for (k, o) in out.iter_mut().enumerate().take(64) {
            let (re, im) = m.row(k);
            let (mut a, mut b) = (0.0f32, 0.0f32);
            for n in 0..128 {
                a += u[n] * re[n];
                b += u[n] * im[n];
            }
            *o = Cplx::new(a, b);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snr(reference: &[f64], got: &[f64]) -> f64 {
        let s: f64 = reference.iter().map(|v| v * v).sum();
        let e: f64 = reference.iter().zip(got).map(|(a, b)| (a - b) * (a - b)).sum();
        10.0 * (s / e.max(1e-300)).log10()
    }

    /// The flowcharts evaluated literally in double precision, as the
    /// reference the fast paths are checked against.
    fn reference_analysis32(x: &[f64; 320]) -> Vec<(f64, f64)> {
        let mut u = [0.0f64; 64];
        for n in 0..64 {
            for j in 0..5 {
                u[n] += x[n + 64 * j] * QMF_WINDOW[2 * (n + 64 * j)];
            }
        }
        (0..32)
            .map(|k| {
                let (mut re, mut im) = (0.0, 0.0);
                for (n, &un) in u.iter().enumerate() {
                    let p = PI / 64.0 * (k as f64 + 0.5) * (2.0 * n as f64 - 0.5);
                    re += un * 2.0 * p.cos();
                    im += un * 2.0 * p.sin();
                }
                (re, im)
            })
            .collect()
    }

    fn signal(n: usize, rate: f64) -> Vec<f32> {
        (0..n)
            .map(|i| {
                let t = i as f64 / rate;
                (0.5 * (2.0 * PI * 997.0 * t).sin() + 0.3 * (2.0 * PI * 5003.0 * t + 1.0).sin()
                    + 0.1 * (((i * 7919) % 101) as f64 / 50.0 - 1.0)) as f32
            })
            .collect()
    }

    #[test]
    fn analysis32_matches_the_flowchart_in_double_precision() {
        let input = signal(32 * 40, 22_050.0);
        let mut bank = Analysis32::default();
        let mut x = [0.0f64; 320];
        let mut out = [Cplx::ZERO; 32];
        let mut worst = f64::INFINITY;
        for slot in input.chunks(32) {
            bank.process(slot, &mut out);
            x.copy_within(0..288, 32);
            for (n, &s) in slot.iter().enumerate() {
                x[31 - n] = f64::from(s);
            }
            let want = reference_analysis32(&x);
            let a: Vec<f64> = want.iter().flat_map(|&(r, i)| [r, i]).collect();
            let b: Vec<f64> = out.iter().flat_map(|c| [f64::from(c.re), f64::from(c.im)]).collect();
            if a.iter().any(|v| v.abs() > 1e-3) {
                worst = worst.min(snr(&a, &b));
            }
        }
        assert!(worst > 110.0, "{worst:.1} dB");
    }

    fn tones(n: usize, rate: f64, delay: f64) -> Vec<f64> {
        (0..n)
            .map(|i| {
                let t = (i as f64 - delay) / rate;
                0.5 * (2.0 * PI * 997.0 * t).sin() + 0.3 * (2.0 * PI * 5003.0 * t + 1.0).sin()
            })
            .collect()
    }

    /// Run `tones` at `rate_in` through `chain` (one slot of `n_in` samples
    /// in, `n_out` out) and find the delay at which the output best matches
    /// the same tones at the output rate: `(delay, gain, snr)`.
    fn measure(rate_in: f64, n_in: usize, n_out: usize, mut chain: impl FnMut(&[f32], &mut [f32])) -> (usize, f64, f64) {
        let slots = 300;
        let input: Vec<f32> = tones(n_in * slots, rate_in, 0.0).iter().map(|&v| v as f32).collect();
        let mut out = vec![0.0f32; n_out * slots];
        for (i, o) in input.chunks(n_in).zip(out.chunks_mut(n_out)) {
            chain(i, o);
        }
        let rate_out = rate_in * n_out as f64 / n_in as f64;
        let got: Vec<f64> = out[n_out * 100..].iter().map(|&v| f64::from(v)).collect();
        (0..2000)
            .map(|d| {
                let want = &tones(n_out * slots, rate_out, d as f64)[n_out * 100..];
                let gain = got.iter().zip(want).map(|(g, w)| g * w).sum::<f64>()
                    / want.iter().map(|w| w * w).sum::<f64>();
                let scaled: Vec<f64> = want.iter().map(|w| w * gain).collect();
                (d, gain, snr(&scaled, &got))
            })
            .max_by(|a, b| a.2.total_cmp(&b.2))
            .unwrap()
    }

    /// Every analysis / synthesis pair the codec uses reconstructs its
    /// input at unity gain, delayed: the decoder's own pairs (the 32-band
    /// analysis with the downsampled and the full synthesis, the SBR tool's
    /// upsampling path) to the window's near-perfect reconstruction, and the
    /// encoder's 64-band analysis with both syntheses.
    #[test]
    fn analysis_then_synthesis_reconstructs_at_unity_gain() {
        let mut sub = [Cplx::ZERO; 64];
        let (mut a32, mut a64) = (Analysis32::default(), Analysis64::default());
        let (mut s32, mut s64) = (Synthesis32::default(), Synthesis64::default());
        let pairs = [
            ("a32 s32", measure(16_000.0, 32, 32, |i, o| { a32.process(i, &mut sub[..32]); s32.process(&sub, o); }), 289, 70.0),
            ("a32 s64", measure(16_000.0, 32, 64, |i, o| { a32.process(i, &mut sub[..32]); s64.process(&sub, o); }), 578, 70.0),
            ("a64 s64", measure(32_000.0, 64, 64, |i, o| { a64.process(i, &mut sub); s64.process(&sub, o); }), 576, 58.0),
            ("a64 s32", measure(32_000.0, 64, 32, |i, o| { a64.process(i, &mut sub); s32.process(&sub, o); }), 288, 58.0),
        ];
        for (name, (delay, gain, snr), want_delay, want_snr) in pairs {
            assert_eq!(delay, want_delay, "{name}");
            assert!((gain - 1.0).abs() < 1e-3, "{name}: gain {gain}");
            assert!(snr > want_snr, "{name}: {snr:.1} dB");
        }
    }
}
