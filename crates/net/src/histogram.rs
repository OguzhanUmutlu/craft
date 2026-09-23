use serde::{Deserialize, Serialize};
use std::fmt;

/// Number of logarithmic decades tracked (from 1 us up to 60+ seconds).
pub const NUM_DECADES: usize = 8;
/// Sub-buckets per decade for high quantization accuracy (1x to 9x base).
pub const SUB_BUCKETS: usize = 9;
/// Total number of histogram buckets (72 buckets).
pub const TOTAL_BUCKETS: usize = NUM_DECADES * SUB_BUCKETS;

/// A high-resolution logarithmic latency micro-histogram.
/// Bounded memory overhead (< 2 KB), constant O(1) sample insertion.
/// Tracks latencies in microseconds (us) from 1 us to 60,000,000 us (60 seconds).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyHistogram {
    /// Count of recorded samples in each bucket.
    pub bucket_counts: Vec<u64>,
    /// Minimum observed latency in microseconds.
    pub min_us: u64,
    /// Maximum observed latency in microseconds.
    pub max_us: u64,
    /// Total count of all recorded samples.
    pub total_count: u64,
    /// Cumulative sum of all recorded samples in microseconds.
    pub sum_us: u64,
}

impl Default for LatencyHistogram {
    fn default() -> Self {
        Self::new()
    }
}

impl LatencyHistogram {
    /// Creates a new empty `LatencyHistogram`.
    pub fn new() -> Self {
        Self {
            bucket_counts: vec![0; TOTAL_BUCKETS],
            min_us: u64::MAX,
            max_us: 0,
            total_count: 0,
            sum_us: 0,
        }
    }

    /// Records a latency sample in microseconds.
    pub fn record_us(&mut self, us: u64) {
        let us = us.max(1);
        let idx = Self::value_to_bucket(us);
        self.bucket_counts[idx] = self.bucket_counts[idx].saturating_add(1);
        self.total_count = self.total_count.saturating_add(1);
        self.sum_us = self.sum_us.saturating_add(us);

        if us < self.min_us {
            self.min_us = us;
        }
        if us > self.max_us {
            self.max_us = us;
        }
    }

    /// Records a latency sample in milliseconds (converted to microseconds).
    pub fn record_ms(&mut self, ms: f64) {
        let us = (ms.max(0.001) * 1000.0).round() as u64;
        self.record_us(us);
    }

    /// Clears all recorded samples.
    pub fn reset(&mut self) {
        self.bucket_counts.fill(0);
        self.min_us = u64::MAX;
        self.max_us = 0;
        self.total_count = 0;
        self.sum_us = 0;
    }

    /// Computes the bucket index for a given microsecond value.
    pub fn value_to_bucket(us: u64) -> usize {
        let us = us.max(1);
        let mut decade = 0usize;
        let mut base = 1u64;

        while decade < NUM_DECADES - 1 && us >= base * 10 {
            base *= 10;
            decade += 1;
        }

        let step = base;
        let sub = (us / step).saturating_sub(1).min((SUB_BUCKETS - 1) as u64) as usize;
        let idx = decade * SUB_BUCKETS + sub;
        idx.min(TOTAL_BUCKETS - 1)
    }

    /// Returns the lower and upper bounds of a bucket in microseconds: `(low_us, high_us)`.
    pub fn bucket_bounds(idx: usize) -> (u64, u64) {
        let idx = idx.min(TOTAL_BUCKETS - 1);
        let decade = idx / SUB_BUCKETS;
        let sub = idx % SUB_BUCKETS;

        let base = 10u64.saturating_pow(decade as u32);
        let low = ((sub as u64) + 1) * base;
        let high = if sub < SUB_BUCKETS - 1 {
            ((sub as u64) + 2) * base
        } else {
            10 * base
        };
        (low, high)
    }

    /// Returns the arithmetic mean of all samples in microseconds.
    pub fn mean_us(&self) -> f64 {
        if self.total_count == 0 {
            0.0
        } else {
            self.sum_us as f64 / self.total_count as f64
        }
    }

    /// Returns the arithmetic mean in milliseconds.
    pub fn mean_ms(&self) -> f64 {
        self.mean_us() / 1000.0
    }

    /// Estimates the value at a specified quantile (0.0 to 1.0) in microseconds.
    /// Uses linear interpolation across the quantile's containing bucket.
    pub fn quantile_us(&self, q: f64) -> f64 {
        if self.total_count == 0 {
            return 0.0;
        }
        let q = q.clamp(0.0, 1.0);
        let target_rank = (q * self.total_count as f64).ceil() as u64;
        let target_rank = target_rank.max(1);

        let mut cumulative = 0u64;
        for (idx, &count) in self.bucket_counts.iter().enumerate() {
            if count == 0 {
                continue;
            }
            let prev_cum = cumulative;
            cumulative += count;

            if cumulative >= target_rank {
                let (low, high) = Self::bucket_bounds(idx);
                let rank_in_bucket = target_rank.saturating_sub(prev_cum);
                let fraction = rank_in_bucket as f64 / count as f64;
                let interpolated = low as f64 + fraction * (high - low) as f64;
                let actual_min = if self.min_us == u64::MAX { 0.0 } else { self.min_us as f64 };
                return interpolated.clamp(actual_min, self.max_us as f64);
            }
        }

        self.max_us as f64
    }

    /// Estimates the value at a specified quantile in milliseconds.
    pub fn quantile_ms(&self, q: f64) -> f64 {
        self.quantile_us(q) / 1000.0
    }

    /// Returns the P50 median in milliseconds.
    pub fn p50_ms(&self) -> f64 {
        self.quantile_ms(0.50)
    }

    /// Returns the P90 in milliseconds.
    pub fn p90_ms(&self) -> f64 {
        self.quantile_ms(0.90)
    }

    /// Returns the P95 in milliseconds.
    pub fn p95_ms(&self) -> f64 {
        self.quantile_ms(0.95)
    }

    /// Returns the P99 in milliseconds.
    pub fn p99_ms(&self) -> f64 {
        self.quantile_ms(0.99)
    }

    /// Returns the P99.9 in milliseconds.
    pub fn p999_ms(&self) -> f64 {
        self.quantile_ms(0.999)
    }

    /// Formats a duration in microseconds to a clean human-readable unit string.
    pub fn format_duration_us(us: u64) -> String {
        if us < 1_000 {
            format!("{} us", us)
        } else if us < 1_000_000 {
            format!("{:.2} ms", us as f64 / 1_000.0)
        } else {
            format!("{:.2} s", us as f64 / 1_000_000.0)
        }
    }

    /// Generates ASCII bar chart lines representing the non-empty buckets.
    pub fn render_ascii(&self, max_width: usize) -> Vec<String> {
        let mut lines = Vec::new();
        if self.total_count == 0 {
            lines.push("[EMPTY] No latency samples recorded yet.".to_string());
            return lines;
        }

        let max_count = *self.bucket_counts.iter().max().unwrap_or(&1);
        let bar_width = max_width.saturating_sub(35).max(10).min(40);

        for (idx, &count) in self.bucket_counts.iter().enumerate() {
            if count == 0 {
                continue;
            }
            let (low, high) = Self::bucket_bounds(idx);
            let label = format!("{:<9} - {:<9}", Self::format_duration_us(low), Self::format_duration_us(high));
            let filled = ((count as f64 / max_count as f64) * bar_width as f64).round() as usize;
            let bar: String = "#".repeat(filled);
            let pct = (count as f64 / self.total_count as f64) * 100.0;

            lines.push(format!("  {} | {:<width$} | {:>6} ({:>5.1}%)", label, bar, count, pct, width = bar_width));
        }

        lines
    }
}

impl fmt::Display for LatencyHistogram {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.total_count == 0 {
            return write!(f, "LatencyHistogram(count: 0)");
        }
        write!(
            f,
            "LatencyHistogram(count: {}, min: {:.2}ms, mean: {:.2}ms, p50: {:.2}ms, p90: {:.2}ms, p99: {:.2}ms, p99.9: {:.2}ms, max: {:.2}ms)",
            self.total_count,
            self.min_us as f64 / 1000.0,
            self.mean_ms(),
            self.p50_ms(),
            self.p90_ms(),
            self.p99_ms(),
            self.p999_ms(),
            self.max_us as f64 / 1000.0
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_histogram_basic_recording_and_quantiles() {
        let mut hist = LatencyHistogram::new();
        assert_eq!(hist.total_count, 0);

        // Record 100 samples uniformly from 1ms to 100ms
        for ms in 1..=100 {
            hist.record_ms(ms as f64);
        }

        assert_eq!(hist.total_count, 100);
        assert!(hist.min_us >= 1000);
        assert!(hist.max_us <= 100_000);
        assert!((hist.mean_ms() - 50.5).abs() < 1.0);

        // P50 should be around 50ms (+- 5ms bucket resolution)
        let p50 = hist.p50_ms();
        assert!((p50 - 50.0).abs() < 6.0, "p50 was {}", p50);

        // P90 should be around 90ms
        let p90 = hist.p90_ms();
        assert!((p90 - 90.0).abs() < 8.0, "p90 was {}", p90);

        // P99 should be around 99ms
        let p99 = hist.p99_ms();
        assert!((p99 - 99.0).abs() < 8.0, "p99 was {}", p99);
    }

    #[test]
    fn test_histogram_sub_microsecond_and_edge_cases() {
        let mut hist = LatencyHistogram::new();
        hist.record_us(5);
        hist.record_us(50);
        hist.record_us(500);
        hist.record_us(5_000);
        hist.record_us(50_000);
        hist.record_us(500_000);

        assert_eq!(hist.total_count, 6);
        assert_eq!(hist.min_us, 5);
        assert_eq!(hist.max_us, 500_000);

        let lines = hist.render_ascii(80);
        assert!(!lines.is_empty());
        assert!(lines[0].contains("|"));
    }

    #[test]
    fn test_bucket_bounds_roundtrip() {
        for idx in 0..TOTAL_BUCKETS {
            let (low, high) = LatencyHistogram::bucket_bounds(idx);
            assert!(low < high, "bucket {} bounds invalid: {} - {}", idx, low, high);
            let mapped = LatencyHistogram::value_to_bucket(low);
            assert_eq!(mapped, idx, "value {} mapped to {} instead of {}", low, mapped, idx);
        }
    }
}
