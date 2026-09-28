//! The filterbank's transforms: the encoder's MDCT and the decoder's IMDCT,
//! both computed through a quarter-length complex FFT, and the window
//! sequences of ISO/IEC 13818-7 subclause 15.3.2.
//!
//! The MDCT is the encoder's transform of Annex C.3.1.2,
//!
//! ```text
//! X[k] = 2 * sum_{n=0}^{N-1} z[n] cos(2*pi/N * (n + n0) * (k + 1/2)),  n0 = (N/2 + 1)/2
//! ```
//!
//! which is the exact inverse (with the TDAC overlap-add) of the decoder's
//! IMDCT in subclause 15.3.1, so a spectrum computed from 16-bit-scaled PCM
//! decodes back to the same scale. The fast path is the textbook reduction:
//! fold the N windowed inputs into a length-N/2 DCT-IV, and evaluate that
//! DCT-IV with one complex FFT of length N/4 between a pre- and a
//! post-twiddle (e.g. Britanak, Yip & Rao, *Discrete Cosine and Sine
//! Transforms*, 2007, ch. 5; Malvar, *Signal Processing with Lapped
//! Transforms*, 1992).

use std::f64::consts::PI;

/// `window_sequence` (Table 44).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WindowSequence {
    OnlyLong = 0,
    LongStart = 1,
    EightShort = 2,
    LongStop = 3,
}

impl WindowSequence {
    pub fn is_short(self) -> bool {
        self == WindowSequence::EightShort
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Cpx {
    re: f64,
    im: f64,
}

impl Cpx {
    fn mul(self, o: Cpx) -> Cpx {
        Cpx {
            re: self.re * o.re - self.im * o.im,
            im: self.re * o.im + self.im * o.re,
        }
    }
}

/// Iterative radix-2 decimation-in-time FFT of a fixed power-of-two size.
struct Fft {
    n: usize,
    /// `exp(-2*pi*i*k/n)` for `k < n/2`.
    twiddles: Vec<Cpx>,
    bitrev: Vec<usize>,
}

impl Fft {
    fn new(n: usize) -> Self {
        assert!(n.is_power_of_two() && n >= 2);
        let bits = n.trailing_zeros();
        let twiddles = (0..n / 2)
            .map(|k| {
                let a = -2.0 * PI * k as f64 / n as f64;
                Cpx {
                    re: a.cos(),
                    im: a.sin(),
                }
            })
            .collect();
        let bitrev = (0..n)
            .map(|i| i.reverse_bits() >> (usize::BITS - bits))
            .collect();
        Self {
            n,
            twiddles,
            bitrev,
        }
    }

    fn run(&self, data: &mut [Cpx]) {
        let n = self.n;
        for i in 0..n {
            let j = self.bitrev[i];
            if j > i {
                data.swap(i, j);
            }
        }
        let mut len = 2;
        while len <= n {
            let half = len / 2;
            let step = n / len;
            for start in (0..n).step_by(len) {
                for k in 0..half {
                    let w = self.twiddles[k * step];
                    let a = data[start + k];
                    let b = data[start + k + half].mul(w);
                    data[start + k] = Cpx {
                        re: a.re + b.re,
                        im: a.im + b.im,
                    };
                    data[start + k + half] = Cpx {
                        re: a.re - b.re,
                        im: a.im - b.im,
                    };
                }
            }
            len *= 2;
        }
    }
}

/// Forward MDCT of `2m` windowed inputs to `m` coefficients, and its inverse.
pub(crate) struct Mdct {
    m: usize,
    fft: Fft,
    /// `exp(-i*pi*j/m)` for `j < m/2`.
    pre: Vec<Cpx>,
    /// `exp(-i*pi*(k + 1/4)/m)` for `k < m/2`.
    post: Vec<Cpx>,
    fold: Vec<f64>,
    work: Vec<Cpx>,
}

impl Mdct {
    pub fn new(m: usize) -> Self {
        let tw = |a: f64| Cpx {
            re: a.cos(),
            im: a.sin(),
        };
        Self {
            m,
            fft: Fft::new(m / 2),
            pre: (0..m / 2).map(|j| tw(-PI * j as f64 / m as f64)).collect(),
            post: (0..m / 2)
                .map(|k| tw(-PI * (k as f64 + 0.25) / m as f64))
                .collect(),
            fold: vec![0.0; m],
            work: vec![Cpx::default(); m / 2],
        }
    }

    /// `input.len() == 2m` (already windowed), `out.len() == m`.
    pub fn forward(&mut self, input: &[f32], out: &mut [f32]) {
        let m = self.m;
        let h = m / 2;
        debug_assert_eq!(input.len(), 2 * m);
        debug_assert_eq!(out.len(), m);
        // Split the input in quarters a | b | c | d; the MDCT equals the
        // DCT-IV of (-c_reversed - d, a - b_reversed).
        for n in 0..h {
            self.fold[n] = -f64::from(input[3 * h - 1 - n]) - f64::from(input[3 * h + n]);
            self.fold[h + n] = f64::from(input[n]) - f64::from(input[m - 1 - n]);
        }
        self.dct4();
        // The factor 2 of the standard's definition.
        for (o, &v) in out.iter_mut().zip(&self.fold) {
            *o = (2.0 * v) as f32;
        }
    }

    /// The decoder's IMDCT (subclause 15.3.1) of `m` coefficients to `2m`
    /// unwindowed samples,
    ///
    /// ```text
    /// x[n] = 2/N * sum_{k=0}^{N/2-1} spec[k] cos(2*pi/N * (n + n0) * (k + 1/2)),  N = 2m
    /// ```
    ///
    /// which is the DCT-IV of the spectrum, `u = DCT-IV(spec) / m`, unfolded
    /// into the time-domain alias pattern `(u2, -rev(u2), -rev(u1), -u1)`.
    pub fn inverse(&mut self, spec: &[f32], out: &mut [f32]) {
        let m = self.m;
        let h = m / 2;
        debug_assert_eq!(spec.len(), m);
        debug_assert_eq!(out.len(), 2 * m);
        for (f, &x) in self.fold.iter_mut().zip(spec) {
            *f = f64::from(x);
        }
        self.dct4();
        let scale = 1.0 / m as f64;
        let u = &self.fold;
        for n in 0..h {
            out[n] = (u[h + n] * scale) as f32;
            out[h + n] = (-u[m - 1 - n] * scale) as f32;
            out[m + n] = (-u[h - 1 - n] * scale) as f32;
            out[m + h + n] = (-u[n] * scale) as f32;
        }
    }

    /// `fold <- DCT-IV(fold)`, `X[k] = sum_n v[n] cos(pi/m * (n + 1/2) * (k + 1/2))`,
    /// through an m/2-point complex FFT: with `c[j] = v[2j] + i*v[m-1-2j]`,
    /// `X[2k] - i*X[m-1-2k] = post[k] * FFT(c[j] * pre[j])[k]`.
    fn dct4(&mut self) {
        let m = self.m;
        let h = m / 2;
        for j in 0..h {
            let c = Cpx {
                re: self.fold[2 * j],
                im: self.fold[m - 1 - 2 * j],
            };
            self.work[j] = c.mul(self.pre[j]);
        }
        self.fft.run(&mut self.work);
        for k in 0..h {
            let y = self.work[k].mul(self.post[k]);
            self.fold[2 * k] = y.re;
            self.fold[m - 1 - 2 * k] = -y.im;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn direct_mdct(z: &[f32]) -> Vec<f64> {
        let n = z.len();
        let n0 = (n as f64 / 2.0 + 1.0) / 2.0;
        (0..n / 2)
            .map(|k| {
                2.0 * z
                    .iter()
                    .enumerate()
                    .map(|(i, &v)| {
                        f64::from(v)
                            * (2.0 * PI / n as f64 * (i as f64 + n0) * (k as f64 + 0.5)).cos()
                    })
                    .sum::<f64>()
            })
            .collect()
    }

    /// The decoder's IMDCT of subclause 15.3.1.
    fn direct_imdct(spec: &[f32]) -> Vec<f64> {
        let n = spec.len() * 2;
        let n0 = (n as f64 / 2.0 + 1.0) / 2.0;
        (0..n)
            .map(|i| {
                2.0 / n as f64
                    * spec
                        .iter()
                        .enumerate()
                        .map(|(k, &x)| {
                            f64::from(x)
                                * (2.0 * PI / n as f64 * (i as f64 + n0) * (k as f64 + 0.5)).cos()
                        })
                        .sum::<f64>()
            })
            .collect()
    }

    fn noise(len: usize, seed: u32) -> Vec<f32> {
        let mut s = seed;
        (0..len)
            .map(|_| {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (s >> 8) as f32 / (1 << 24) as f32 * 2.0 - 1.0
            })
            .collect()
    }

    #[test]
    fn fast_mdct_matches_the_definition() {
        for m in [128usize, 1024] {
            let z = noise(2 * m, m as u32);
            let mut fast = vec![0.0f32; m];
            Mdct::new(m).forward(&z, &mut fast);
            let reference = direct_mdct(&z);
            let peak = reference.iter().fold(0.0f64, |a, &b| a.max(b.abs()));
            for (a, b) in fast.iter().zip(&reference) {
                assert!((f64::from(*a) - b).abs() < peak * 1e-5, "{m}: {a} vs {b}");
            }
        }
    }

    #[test]
    fn fast_imdct_matches_the_definition() {
        for m in [128usize, 1024] {
            let spec = noise(m, 3 + m as u32);
            let mut fast = vec![0.0f32; 2 * m];
            Mdct::new(m).inverse(&spec, &mut fast);
            for (n, (&a, b)) in fast.iter().zip(direct_imdct(&spec)).enumerate() {
                assert!((f64::from(a) - b).abs() < 1e-5, "{m}: {n}: {a} vs {b}");
            }
        }
    }

    #[test]
    fn windowed_overlap_add_reconstructs_across_every_transition() {
        // ONLY_LONG -> LONG_START -> EIGHT_SHORT -> LONG_STOP -> ONLY_LONG,
        // analysed here and synthesised with the standard's IMDCT, windows and
        // overlap-add: the middle frames must come back sample for sample.
        let long = crate::tables::windows::sine(2048);
        let short = crate::tables::windows::sine(256);
        let seqs = [
            WindowSequence::OnlyLong,
            WindowSequence::LongStart,
            WindowSequence::EightShort,
            WindowSequence::LongStop,
            WindowSequence::OnlyLong,
        ];
        let x = noise(1024 * (seqs.len() + 1), 7);
        let mut out = vec![0.0f64; x.len()];
        let mut long_mdct = Mdct::new(1024);
        let mut short_mdct = Mdct::new(128);
        for (f, &seq) in seqs.iter().enumerate() {
            let frame = &x[f * 1024..f * 1024 + 2048];
            if seq.is_short() {
                for j in 0..8 {
                    let at = 448 + 128 * j;
                    let z: Vec<f32> = (0..256).map(|n| frame[at + n] * short[n]).collect();
                    let mut spec = vec![0.0f32; 128];
                    short_mdct.forward(&z, &mut spec);
                    let y = direct_imdct(&spec);
                    for n in 0..256 {
                        out[f * 1024 + at + n] += y[n] * f64::from(short[n]);
                    }
                }
            } else {
                let w = crate::encode::long_window(seq, &long, &short);
                let z: Vec<f32> = frame.iter().zip(&w).map(|(a, b)| a * b).collect();
                let mut spec = vec![0.0f32; 1024];
                long_mdct.forward(&z, &mut spec);
                let y = direct_imdct(&spec);
                for n in 0..2048 {
                    out[f * 1024 + n] += y[n] * f64::from(w[n]);
                }
            }
        }
        for i in 1024..1024 * seqs.len() {
            assert!(
                (out[i] - f64::from(x[i])).abs() < 1e-4,
                "sample {i}: {} vs {}",
                out[i],
                x[i]
            );
        }
    }
}
