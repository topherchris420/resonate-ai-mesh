//! Descriptive statistics and paired comparisons.
//!
//! Experiments use common random numbers: repetition `r` of every condition
//! runs with the same seed, so conditions are compared pairwise and the
//! difference isolates the manipulated variable. Intervals are 95% unless
//! stated. Bootstrap resampling is seeded, so reports are reproducible.

use crate::rng::Rng;
use event_bus::quantize;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Descriptive {
    pub n: usize,
    pub mean: f64,
    pub median: f64,
    /// Sample standard deviation (n - 1); null for n < 2.
    pub sd: Option<f64>,
    pub variance: Option<f64>,
    pub min: f64,
    pub max: f64,
    /// t-based 95% interval for the mean; null for n < 2.
    pub ci95: Option<[f64; 2]>,
}

fn r(value: f64) -> f64 {
    quantize(value, 6)
}

/// Two-sided 97.5% Student t quantiles.
pub fn t975(df: usize) -> f64 {
    const TABLE: [f64; 30] = [
        12.706, 4.303, 3.182, 2.776, 2.571, 2.447, 2.365, 2.306, 2.262, 2.228, 2.201, 2.179, 2.160,
        2.145, 2.131, 2.120, 2.110, 2.101, 2.093, 2.086, 2.080, 2.074, 2.069, 2.064, 2.060, 2.056,
        2.052, 2.048, 2.045, 2.042,
    ];
    match df {
        0 => f64::NAN,
        1..=30 => TABLE[df - 1],
        31..=40 => 2.021,
        41..=60 => 2.000,
        61..=80 => 1.990,
        81..=100 => 1.984,
        101..=120 => 1.980,
        _ => 1.960,
    }
}

pub fn median(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

pub fn describe(values: &[f64]) -> Option<Descriptive> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let n = values.len();
    let mean = values.iter().sum::<f64>() / n as f64;
    let (sd, variance, ci95) = if n >= 2 {
        let variance = values.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / (n - 1) as f64;
        let sd = variance.sqrt();
        let half = t975(n - 1) * sd / (n as f64).sqrt();
        (
            Some(r(sd)),
            Some(r(variance)),
            Some([r(mean - half), r(mean + half)]),
        )
    } else {
        (None, None, None)
    };
    Some(Descriptive {
        n,
        mean: r(mean),
        median: r(median(&sorted)),
        sd,
        variance,
        min: r(sorted[0]),
        max: r(sorted[n - 1]),
        ci95,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PairedComparison {
    pub metric: String,
    pub baseline: String,
    pub treatment: String,
    pub n_pairs: usize,
    pub baseline_mean: f64,
    pub treatment_mean: f64,
    /// Mean of (treatment - baseline) over paired repetitions.
    pub mean_difference: f64,
    pub median_difference: f64,
    pub sd_difference: Option<f64>,
    /// t-based 95% interval for the mean paired difference.
    pub ci95_t: Option<[f64; 2]>,
    /// Seeded percentile bootstrap (4000 resamples) 95% interval.
    pub ci95_bootstrap: Option<[f64; 2]>,
    /// Standardized paired difference: mean difference / sd of differences.
    pub cohens_dz: Option<f64>,
    /// Between-condition standardized difference with small-sample correction.
    pub hedges_g: Option<f64>,
    pub pairs_increased: usize,
    pub pairs_decreased: usize,
    pub pairs_equal: usize,
}

pub fn paired(
    metric: &str,
    baseline: &str,
    treatment: &str,
    pairs: &[(f64, f64)],
) -> Option<PairedComparison> {
    if pairs.is_empty() {
        return None;
    }
    let differences: Vec<f64> = pairs.iter().map(|(b, t)| t - b).collect();
    let summary = describe(&differences)?;
    let base: Vec<f64> = pairs.iter().map(|p| p.0).collect();
    let treat: Vec<f64> = pairs.iter().map(|p| p.1).collect();
    let base_d = describe(&base)?;
    let treat_d = describe(&treat)?;
    let n = pairs.len();
    let cohens_dz = match summary.sd {
        Some(sd) if sd > 0.0 => Some(r(summary.mean / sd)),
        _ => None,
    };
    let hedges_g = match (base_d.variance, treat_d.variance) {
        (Some(vb), Some(vt)) if n >= 2 => {
            let pooled = ((vb + vt) / 2.0).sqrt();
            if pooled > 0.0 {
                let correction = 1.0 - 3.0 / (4.0 * (2 * n) as f64 - 9.0);
                Some(r((treat_d.mean - base_d.mean) / pooled * correction))
            } else {
                None
            }
        }
        _ => None,
    };
    let ci95_bootstrap = bootstrap_mean_ci(&differences, 4000, metric);
    Some(PairedComparison {
        metric: metric.to_string(),
        baseline: baseline.to_string(),
        treatment: treatment.to_string(),
        n_pairs: n,
        baseline_mean: base_d.mean,
        treatment_mean: treat_d.mean,
        mean_difference: summary.mean,
        median_difference: summary.median,
        sd_difference: summary.sd,
        ci95_t: summary.ci95,
        ci95_bootstrap,
        cohens_dz,
        hedges_g,
        pairs_increased: differences.iter().filter(|d| **d > 0.0).count(),
        pairs_decreased: differences.iter().filter(|d| **d < 0.0).count(),
        pairs_equal: differences.iter().filter(|d| **d == 0.0).count(),
    })
}

pub fn bootstrap_mean_ci(values: &[f64], resamples: usize, label: &str) -> Option<[f64; 2]> {
    if values.len() < 2 {
        return None;
    }
    let mut rng = Rng::stream(0x5eed, &format!("bootstrap.{label}"));
    let n = values.len();
    let mut means: Vec<f64> = (0..resamples)
        .map(|_| {
            (0..n)
                .map(|_| values[rng.below(n as u64) as usize])
                .sum::<f64>()
                / n as f64
        })
        .collect();
    means.sort_by(f64::total_cmp);
    let low = means[((resamples as f64) * 0.025).floor() as usize];
    let high = means[(((resamples as f64) * 0.975).ceil() as usize).min(resamples - 1)];
    Some([r(low), r(high)])
}

/// Wilson score 95% interval for a proportion k/n.
pub fn wilson(k: usize, n: usize) -> Option<[f64; 2]> {
    if n == 0 {
        return None;
    }
    let z = 1.959_963_985;
    let n_f = n as f64;
    let p = k as f64 / n_f;
    let denominator = 1.0 + z * z / n_f;
    let center = (p + z * z / (2.0 * n_f)) / denominator;
    let half = z * ((p * (1.0 - p) / n_f + z * z / (4.0 * n_f * n_f)).sqrt()) / denominator;
    Some([r((center - half).max(0.0)), r((center + half).min(1.0))])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_matches_hand_calculation() {
        let d = describe(&[2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]).unwrap();
        assert_eq!(d.mean, 5.0);
        assert_eq!(d.median, 4.5);
        assert!((d.variance.unwrap() - 4.571429).abs() < 1e-6);
        let ci = d.ci95.unwrap();
        assert!(ci[0] < 5.0 && ci[1] > 5.0);
        assert_eq!(describe(&[3.0]).unwrap().sd, None);
        assert!(describe(&[]).is_none());
    }

    #[test]
    fn paired_comparison_reports_direction_and_effect() {
        let pairs: Vec<(f64, f64)> = (0..20)
            .map(|i| {
                (
                    10.0 + i as f64 * 0.1,
                    8.0 + i as f64 * 0.1 + (i % 3) as f64 * 0.2,
                )
            })
            .collect();
        let c = paired("m", "a", "b", &pairs).unwrap();
        assert!(c.mean_difference < 0.0);
        assert!(c.ci95_t.unwrap()[1] < 0.0);
        assert!(c.ci95_bootstrap.unwrap()[1] < 0.0);
        assert!(c.cohens_dz.unwrap() < 0.0);
        assert_eq!(c.pairs_decreased, 20);
        assert_eq!(
            paired("m", "a", "b", &pairs).unwrap(),
            c,
            "seeded bootstrap is reproducible"
        );
    }

    #[test]
    fn identical_conditions_have_zero_difference_and_no_dz() {
        let pairs: Vec<(f64, f64)> = (0..10).map(|i| (i as f64, i as f64)).collect();
        let c = paired("m", "a", "b", &pairs).unwrap();
        assert_eq!(c.mean_difference, 0.0);
        assert_eq!(c.cohens_dz, None);
        assert_eq!(c.pairs_equal, 10);
    }

    #[test]
    fn wilson_interval_is_bounded() {
        assert_eq!(wilson(0, 0), None);
        let [low, high] = wilson(0, 100).unwrap();
        assert_eq!(low, 0.0);
        assert!(high > 0.0 && high < 0.05);
        let [low, high] = wilson(50, 100).unwrap();
        assert!(low < 0.5 && high > 0.5);
    }
}
