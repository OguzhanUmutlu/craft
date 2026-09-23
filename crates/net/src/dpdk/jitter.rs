use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct JitterCalculator {
    last_timestamp_nanos: Option<u64>,
    sample_count: u64,
    mean_micros: f64,
    m2: f64,
    recent_deltas_micros: VecDeque<f64>,
    window_size: usize,
}

impl Default for JitterCalculator {
    fn default() -> Self {
        Self::new(1000)
    }
}

impl JitterCalculator {
    pub fn new(window_size: usize) -> Self {
        Self {
            last_timestamp_nanos: None,
            sample_count: 0,
            mean_micros: 0.0,
            m2: 0.0,
            recent_deltas_micros: VecDeque::with_capacity(window_size),
            window_size,
        }
    }

    pub fn record_packet(&mut self, timestamp_nanos: u64) -> Option<f64> {
        let delta_micros = if let Some(last) = self.last_timestamp_nanos {
            let diff_nanos = timestamp_nanos.saturating_sub(last);
            let diff_micros = (diff_nanos as f64) / 1000.0;

            // Welford's algorithm
            self.sample_count += 1;
            let delta = diff_micros - self.mean_micros;
            self.mean_micros += delta / (self.sample_count as f64);
            let delta2 = diff_micros - self.mean_micros;
            self.m2 += delta * delta2;

            if self.recent_deltas_micros.len() >= self.window_size {
                self.recent_deltas_micros.pop_front();
            }
            self.recent_deltas_micros.push_back(diff_micros);

            Some(diff_micros)
        } else {
            None
        };

        self.last_timestamp_nanos = Some(timestamp_nanos);
        delta_micros
    }

    pub fn sample_count(&self) -> u64 {
        self.sample_count
    }

    pub fn mean_jitter_micros(&self) -> f64 {
        self.mean_micros
    }

    pub fn std_dev_micros(&self) -> f64 {
        if self.sample_count < 2 {
            0.0
        } else {
            (self.m2 / ((self.sample_count - 1) as f64)).sqrt()
        }
    }

    pub fn quantile_micros(&self, q: f64) -> f64 {
        if self.recent_deltas_micros.is_empty() {
            return 0.0;
        }
        let mut sorted: Vec<f64> = self.recent_deltas_micros.iter().copied().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let idx = ((sorted.len() as f64) * q).clamp(0.0, (sorted.len() - 1) as f64) as usize;
        sorted[idx]
    }

    pub fn p50_micros(&self) -> f64 {
        self.quantile_micros(0.50)
    }

    pub fn p90_micros(&self) -> f64 {
        self.quantile_micros(0.90)
    }

    pub fn p99_micros(&self) -> f64 {
        self.quantile_micros(0.99)
    }

    pub fn render_sparkline(&self, width: usize) -> String {
        if self.recent_deltas_micros.is_empty() {
            return "-".repeat(width);
        }
        let samples: Vec<f64> = self.recent_deltas_micros.iter().copied().collect();
        let min_val = samples.iter().copied().fold(f64::INFINITY, f64::min);
        let max_val = samples.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let range = (max_val - min_val).max(0.001);

        // 8-level ASCII blocks without emojis
        let levels = [' ', '.', ':', '-', '=', '+', '*', '#'];
        let chunk_size = (samples.len() as f64) / (width as f64);
        let mut spark = String::with_capacity(width);

        for i in 0..width {
            let start = (i as f64 * chunk_size) as usize;
            let end = (((i + 1) as f64 * chunk_size) as usize).min(samples.len()).max(start + 1);
            let slice = &samples[start..end];
            let avg = if !slice.is_empty() {
                slice.iter().sum::<f64>() / slice.len() as f64
            } else {
                min_val
            };
            let norm = ((avg - min_val) / range).clamp(0.0, 1.0);
            let idx = (norm * (levels.len() - 1) as f64) as usize;
            spark.push(levels[idx]);
        }

        spark
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jitter_calculation() {
        let mut calc = JitterCalculator::new(100);

        // Feed packets with intervals around 1000 microseconds (1ms)
        let base = 1_000_000_000u64; // 1s
        calc.record_packet(base);
        calc.record_packet(base + 1_000_000); // +1ms
        calc.record_packet(base + 2_050_000); // +1.05ms
        calc.record_packet(base + 3_010_000); // +0.96ms

        assert_eq!(calc.sample_count(), 3);
        assert!(calc.mean_jitter_micros() > 900.0 && calc.mean_jitter_micros() < 1100.0);
        assert!(calc.p99_micros() > 0.0);

        let spark = calc.render_sparkline(10);
        assert_eq!(spark.len(), 10);
    }
}
