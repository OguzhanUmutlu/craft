use crate::histogram::LatencyHistogram;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

/// Single tick sample with timing data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickSample {
    pub timestamp_ms: u64,
    pub duration_ms: f64,
    pub tps: f64,
}

/// Operational health rating based on tick duration and TPS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TickHealthGrade {
    Pristine,
    Stable,
    Degraded,
    Overloaded,
}

impl TickHealthGrade {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pristine => "[PRISTINE]",
            Self::Stable => "[STABLE]",
            Self::Degraded => "[DEGRADED]",
            Self::Overloaded => "[OVERLOADED]",
        }
    }
}

impl fmt::Display for TickHealthGrade {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Snapshot summary of tick profiling statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickProfileSummary {
    pub sample_count: usize,
    pub current_tps: f64,
    pub avg_tps: f64,
    pub current_mspt: f64,
    pub avg_mspt: f64,
    pub min_mspt: f64,
    pub max_mspt: f64,
    pub mspt_p50: f64,
    pub mspt_p90: f64,
    pub mspt_p99: f64,
    pub mspt_p999: f64,
    pub jitter_ms: f64,
    pub health: TickHealthGrade,
}

/// Real-time tick duration profiler and MSPT sliding-window estimator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickProfiler {
    pub max_samples: usize,
    pub samples: VecDeque<TickSample>,
    pub histogram: LatencyHistogram,
}

impl Default for TickProfiler {
    fn default() -> Self {
        Self::new(120)
    }
}

impl TickProfiler {
    /// Creates a new `TickProfiler` with a fixed sliding-window capacity.
    pub fn new(max_samples: usize) -> Self {
        Self {
            max_samples: max_samples.max(10),
            samples: VecDeque::with_capacity(max_samples.max(10)),
            histogram: LatencyHistogram::new(),
        }
    }

    /// Records a tick duration in milliseconds.
    pub fn record_tick(&mut self, duration_ms: f64) {
        let duration_ms = duration_ms.max(0.1);
        let tps = (1000.0 / duration_ms).clamp(0.0, 20.0);
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        if self.samples.len() >= self.max_samples {
            self.samples.pop_front();
        }

        self.samples.push_back(TickSample {
            timestamp_ms,
            duration_ms,
            tps,
        });

        self.histogram.record_ms(duration_ms);
    }

    /// Current count of buffered samples.
    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }

    /// Most recent observed tick duration in milliseconds.
    pub fn current_mspt(&self) -> f64 {
        self.samples.back().map(|s| s.duration_ms).unwrap_or(0.0)
    }

    /// Most recent calculated TPS.
    pub fn current_tps(&self) -> f64 {
        self.samples.back().map(|s| s.tps).unwrap_or(20.0)
    }

    /// Average MSPT across the buffered sliding window.
    pub fn avg_mspt(&self) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let sum: f64 = self.samples.iter().map(|s| s.duration_ms).sum();
        sum / self.samples.len() as f64
    }

    /// Average TPS across the buffered sliding window.
    pub fn avg_tps(&self) -> f64 {
        if self.samples.is_empty() {
            return 20.0;
        }
        let sum: f64 = self.samples.iter().map(|s| s.tps).sum();
        sum / self.samples.len() as f64
    }

    /// Minimum MSPT in the buffered sliding window.
    pub fn min_mspt(&self) -> f64 {
        self.samples
            .iter()
            .map(|s| s.duration_ms)
            .fold(f64::INFINITY, f64::min)
    }

    /// Maximum MSPT in the buffered sliding window.
    pub fn max_mspt(&self) -> f64 {
        self.samples
            .iter()
            .map(|s| s.duration_ms)
            .fold(0.0, f64::max)
    }

    /// P50 median MSPT in milliseconds.
    pub fn mspt_p50(&self) -> f64 {
        self.histogram.p50_ms()
    }

    /// P90 MSPT in milliseconds.
    pub fn mspt_p90(&self) -> f64 {
        self.histogram.p90_ms()
    }

    /// P99 MSPT in milliseconds.
    pub fn mspt_p99(&self) -> f64 {
        self.histogram.p99_ms()
    }

    /// P99.9 MSPT in milliseconds.
    pub fn mspt_p999(&self) -> f64 {
        self.histogram.p999_ms()
    }

    /// Mean absolute consecutive tick jitter in milliseconds.
    pub fn jitter_ms(&self) -> f64 {
        if self.samples.len() < 2 {
            return 0.0;
        }
        let mut diff_sum = 0.0;
        for i in 1..self.samples.len() {
            diff_sum += (self.samples[i].duration_ms - self.samples[i - 1].duration_ms).abs();
        }
        diff_sum / (self.samples.len() - 1) as f64
    }

    /// Evaluates current operational health grade.
    pub fn health_grade(&self) -> TickHealthGrade {
        let avg_m = self.avg_mspt();
        let avg_t = self.avg_tps();

        if avg_m <= 35.0 && avg_t >= 19.9 {
            TickHealthGrade::Pristine
        } else if avg_m <= 50.0 && avg_t >= 19.0 {
            TickHealthGrade::Stable
        } else if avg_m <= 70.0 && avg_t >= 15.0 {
            TickHealthGrade::Degraded
        } else {
            TickHealthGrade::Overloaded
        }
    }

    /// Exports structured summary snapshot.
    pub fn summary(&self) -> TickProfileSummary {
        let min_m = if self.samples.is_empty() { 0.0 } else { self.min_mspt() };
        TickProfileSummary {
            sample_count: self.samples.len(),
            current_tps: self.current_tps(),
            avg_tps: self.avg_tps(),
            current_mspt: self.current_mspt(),
            avg_mspt: self.avg_mspt(),
            min_mspt: min_m,
            max_mspt: self.max_mspt(),
            mspt_p50: self.mspt_p50(),
            mspt_p90: self.mspt_p90(),
            mspt_p99: self.mspt_p99(),
            mspt_p999: self.mspt_p999(),
            jitter_ms: self.jitter_ms(),
            health: self.health_grade(),
        }
    }

    /// Renders an ASCII sparkline of recent MSPT values.
    /// Character scale: `_` (<25ms), `.` (25-35ms), `-` (35-45ms), `=` (45-50ms),
    ///                  `+` (50-65ms), `*` (65-80ms), `#` (80-100ms), `!` (>100ms)
    pub fn render_sparkline(&self, width: usize) -> String {
        if self.samples.is_empty() {
            return "[NO DATA]".to_string();
        }

        let width = width.max(5).min(self.samples.len());
        let skip = self.samples.len().saturating_sub(width);

        let mut sparkline = String::with_capacity(width);
        for s in self.samples.iter().skip(skip) {
            let ch = if s.duration_ms < 25.0 {
                '_'
            } else if s.duration_ms < 35.0 {
                '.'
            } else if s.duration_ms < 45.0 {
                '-'
            } else if s.duration_ms < 50.0 {
                '='
            } else if s.duration_ms < 65.0 {
                '+'
            } else if s.duration_ms < 80.0 {
                '*'
            } else if s.duration_ms < 100.0 {
                '#'
            } else {
                '!'
            };
            sparkline.push(ch);
        }
        sparkline
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tick_profiler_sliding_window_and_health() {
        let mut profiler = TickProfiler::new(20);

        // Record 20 pristine ticks (20ms each = 20 TPS)
        for _ in 0..20 {
            profiler.record_tick(20.0);
        }

        assert_eq!(profiler.sample_count(), 20);
        assert!((profiler.avg_mspt() - 20.0).abs() < 0.1);
        assert!((profiler.avg_tps() - 20.0).abs() < 0.1);
        assert_eq!(profiler.health_grade(), TickHealthGrade::Pristine);

        // Record degraded ticks (60ms each = ~16.6 TPS)
        for _ in 0..20 {
            profiler.record_tick(60.0);
        }
        assert_eq!(profiler.health_grade(), TickHealthGrade::Degraded);
        assert!((profiler.avg_mspt() - 60.0).abs() < 0.1);

        // Overloaded ticks (120ms each)
        for _ in 0..20 {
            profiler.record_tick(120.0);
        }
        assert_eq!(profiler.health_grade(), TickHealthGrade::Overloaded);
    }

    #[test]
    fn test_tick_profiler_sparkline() {
        let mut profiler = TickProfiler::new(50);
        profiler.record_tick(15.0);
        profiler.record_tick(30.0);
        profiler.record_tick(40.0);
        profiler.record_tick(48.0);
        profiler.record_tick(55.0);
        profiler.record_tick(70.0);
        profiler.record_tick(90.0);
        profiler.record_tick(120.0);

        let spark = profiler.render_sparkline(8);
        assert_eq!(spark, "_.-=+*#!");
    }
}
