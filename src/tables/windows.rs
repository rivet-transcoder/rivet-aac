//! The two window shapes of ISO/IEC 13818-7 subclause 15.3.2, computed from
//! their formulas: the sine window (`window_shape` 0) and the Kaiser-Bessel
//! derived window (`window_shape` 1, alpha 4 for 2048 samples, 6 for 256).
//! Each function returns the whole symmetric window of `n` samples; its first
//! half is the shape's `W_LEFT` and its second half `W_RIGHT`.

use std::f64::consts::PI;

/// `W_SIN(n) = sin(pi / N * (n + 1/2))`.
pub fn sine(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| (PI / n as f64 * (i as f64 + 0.5)).sin() as f32)
        .collect()
}

/// The KBD window of length `n` with kernel alpha `alpha`: the square root of
/// the running sum of the Kaiser-Bessel kernel
/// `W'(k) = I0(pi * alpha * sqrt(1 - ((k - N/4) / (N/4))^2)) / I0(pi * alpha)`
/// for `0 <= k <= N/2`, normalised by its total.
pub fn kbd(n: usize, alpha: f64) -> Vec<f32> {
    let quarter = n as f64 / 4.0;
    let kernel: Vec<f64> = (0..=n / 2)
        .map(|k| {
            let r = (k as f64 - quarter) / quarter;
            bessel_i0(PI * alpha * (1.0 - r * r).max(0.0).sqrt())
        })
        .collect();
    let total: f64 = kernel.iter().sum();
    let mut left = Vec::with_capacity(n / 2);
    let mut acc = 0.0;
    for &w in &kernel[..n / 2] {
        acc += w;
        left.push((acc / total).sqrt() as f32);
    }
    let mut w = left.clone();
    w.extend(left.iter().rev());
    w
}

/// The KBD window a transform length uses: alpha 4 for the long window,
/// 6 for the short one.
pub fn kbd_for(n: usize) -> Vec<f32> {
    kbd(n, if n == 256 { 6.0 } else { 4.0 })
}

/// The zeroth-order modified Bessel function of the first kind, from its
/// power series `sum_k ((x/2)^k / k!)^2`.
fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let half = x / 2.0;
    for k in 1..200 {
        term *= half / k as f64;
        let t2 = term * term;
        sum += t2;
        if t2 < sum * 1e-17 {
            break;
        }
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Princen-Bradley: a window that reconstructs perfectly under TDAC has
    /// `w(k)^2 + w(k + N/2)^2 == 1` over its first half.
    #[test]
    fn both_shapes_meet_the_perfect_reconstruction_condition() {
        for n in [256usize, 2048] {
            for w in [sine(n), kbd_for(n)] {
                assert_eq!(w.len(), n);
                for k in 0..n / 2 {
                    let s = f64::from(w[k]).powi(2) + f64::from(w[k + n / 2]).powi(2);
                    assert!((s - 1.0).abs() < 1e-6, "{n}: {k}: {s}");
                }
                for k in 0..n {
                    assert_eq!(w[k], w[n - 1 - k]);
                }
            }
        }
    }

    #[test]
    fn bessel_series_matches_known_values() {
        assert!((bessel_i0(0.0) - 1.0).abs() < 1e-15);
        // I0(1) = 1.2660658777520082, I0(10) = 2815.716628466254.
        assert!((bessel_i0(1.0) - 1.266_065_877_752_008_2).abs() < 1e-12);
        assert!((bessel_i0(10.0) / 2_815.716_628_466_254 - 1.0).abs() < 1e-12);
    }

    #[test]
    fn kbd_is_narrower_than_sine_at_the_edges() {
        let (s, k) = (sine(2048), kbd_for(2048));
        assert!(k[0] < s[0] && k[100] < s[100]);
    }
}
