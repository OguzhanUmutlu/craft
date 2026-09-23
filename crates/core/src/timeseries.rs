use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionResult {
    pub slope: f64,
    pub intercept: f64,
    pub r_squared: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SawtoothMetrics {
    pub drops_count: usize,
    pub avg_rebound_slope: f64,
    pub floor_climb_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSeriesSample {
    pub timestamp: u64,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollingTimeSeries {
    capacity: usize,
    samples: Vec<TimeSeriesSample>,
}

impl RollingTimeSeries {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(2),
            samples: Vec::with_capacity(capacity.max(2)),
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn push(&mut self, timestamp: u64, value: f64) {
        if self.samples.len() >= self.capacity {
            self.samples.remove(0);
        }
        self.samples.push(TimeSeriesSample { timestamp, value });
    }

    pub fn samples(&self) -> &[TimeSeriesSample] {
        &self.samples
    }

    pub fn latest(&self) -> Option<&TimeSeriesSample> {
        self.samples.last()
    }

    pub fn values(&self) -> Vec<f64> {
        self.samples.iter().map(|s| s.value).collect()
    }

    pub fn timestamps(&self) -> Vec<u64> {
        self.samples.iter().map(|s| s.timestamp).collect()
    }

    pub fn mean(&self) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let sum: f64 = self.samples.iter().map(|s| s.value).sum();
        sum / (self.samples.len() as f64)
    }

    pub fn variance(&self) -> f64 {
        let n = self.samples.len();
        if n < 2 {
            return 0.0;
        }
        let mean = self.mean();
        let sum_sq_diff: f64 = self.samples.iter().map(|s| {
            let diff = s.value - mean;
            diff * diff
        }).sum();
        sum_sq_diff / ((n - 1) as f64)
    }

    pub fn std_dev(&self) -> f64 {
        self.variance().sqrt()
    }

    pub fn z_score(&self, value: f64) -> f64 {
        let sd = self.std_dev();
        if sd < 1e-9 {
            0.0
        } else {
            (value - self.mean()) / sd
        }
    }

    pub fn min(&self) -> f64 {
        self.samples
            .iter()
            .map(|s| s.value)
            .fold(f64::INFINITY, f64::min)
    }

    pub fn max(&self) -> f64 {
        self.samples
            .iter()
            .map(|s| s.value)
            .fold(f64::NEG_INFINITY, f64::max)
    }

    pub fn percentile(&self, p: f64) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let mut sorted = self.values();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let p_clamped = p.clamp(0.0, 1.0);
        let rank = p_clamped * ((sorted.len() - 1) as f64);
        let lower = rank.floor() as usize;
        let upper = rank.ceil() as usize;
        if lower == upper {
            sorted[lower]
        } else {
            let weight = rank - (lower as f64);
            sorted[lower] * (1.0 - weight) + sorted[upper] * weight
        }
    }

    pub fn linear_regression(&self) -> Option<RegressionResult> {
        let n = self.samples.len();
        if n < 2 {
            return None;
        }

        let first_t = self.samples[0].timestamp as f64;
        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let mut sum_xx = 0.0;
        let mut sum_xy = 0.0;

        for s in &self.samples {
            let x = (s.timestamp as f64) - first_t;
            let y = s.value;
            sum_x += x;
            sum_y += y;
            sum_xx += x * x;
            sum_xy += x * y;
        }

        let nf = n as f64;
        let denom = nf * sum_xx - sum_x * sum_x;
        if denom.abs() < 1e-9 {
            return None;
        }

        let slope = (nf * sum_xy - sum_x * sum_y) / denom;
        let intercept = (sum_y - slope * sum_x) / nf;

        let y_mean = sum_y / nf;
        let mut ss_tot = 0.0;
        let mut ss_res = 0.0;

        for s in &self.samples {
            let x = (s.timestamp as f64) - first_t;
            let y = s.value;
            let y_pred = slope * x + intercept;
            let d_tot = y - y_mean;
            let d_res = y - y_pred;
            ss_tot += d_tot * d_tot;
            ss_res += d_res * d_res;
        }

        let r_squared = if ss_tot < 1e-9 {
            1.0
        } else {
            (1.0 - (ss_res / ss_tot)).clamp(0.0, 1.0)
        };

        Some(RegressionResult {
            slope,
            intercept,
            r_squared,
        })
    }

    pub fn predict_time_to_limit(&self, current_val: f64, limit_val: f64) -> Option<u64> {
        if current_val >= limit_val {
            return Some(0);
        }
        let reg = self.linear_regression()?;
        if reg.slope <= 1e-6 || reg.r_squared < 0.50 {
            return None;
        }
        let delta = limit_val - current_val;
        let seconds = delta / reg.slope;
        if seconds > 0.0 && seconds.is_finite() {
            Some(seconds.round() as u64)
        } else {
            None
        }
    }

    pub fn exponential_moving_average(&self, alpha: f64) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let a = alpha.clamp(0.01, 0.99);
        let mut ema = self.samples[0].value;
        for s in &self.samples[1..] {
            ema = a * s.value + (1.0 - a) * ema;
        }
        ema
    }

    pub fn detect_sawtooth_pattern(&self) -> Option<SawtoothMetrics> {
        if self.samples.len() < 6 {
            return None;
        }

        let mut drops_count = 0;
        let mut drop_indices = Vec::new();

        for i in 1..self.samples.len() {
            let prev = self.samples[i - 1].value;
            let curr = self.samples[i].value;
            // A sharp drop of at least 15% between consecutive samples
            if prev > 1e-6 && (prev - curr) / prev >= 0.15 {
                drops_count += 1;
                drop_indices.push(i);
            }
        }

        if drops_count < 2 {
            return None;
        }

        // Calculate floor climb rate across trough minima
        let mut floors = Vec::new();
        for &idx in &drop_indices {
            floors.push(self.samples[idx].value);
        }

        let first_floor = floors[0];
        let last_floor = *floors.last().unwrap();
        let floor_delta = last_floor - first_floor;
        let t_span = (self.samples[self.samples.len() - 1].timestamp
            - self.samples[0].timestamp)
            .max(1) as f64;
        let floor_climb_rate = (floor_delta / t_span).max(0.0);

        // Average rebound slope after drops
        let mut rebound_slopes = Vec::new();
        for &idx in &drop_indices {
            if idx + 2 < self.samples.len() {
                let dt = (self.samples[idx + 2].timestamp - self.samples[idx].timestamp).max(1) as f64;
                let dv = self.samples[idx + 2].value - self.samples[idx].value;
                if dv > 0.0 {
                    rebound_slopes.push(dv / dt);
                }
            }
        }

        let avg_rebound_slope = if rebound_slopes.is_empty() {
            0.0
        } else {
            let sum: f64 = rebound_slopes.iter().sum();
            sum / (rebound_slopes.len() as f64)
        };

        Some(SawtoothMetrics {
            drops_count,
            avg_rebound_slope,
            floor_climb_rate,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rolling_window_basics() {
        let mut r = RollingTimeSeries::new(3);
        r.push(10, 1.0);
        r.push(20, 2.0);
        r.push(30, 3.0);
        assert_eq!(r.len(), 3);
        assert_eq!(r.values(), vec![1.0, 2.0, 3.0]);

        r.push(40, 4.0);
        assert_eq!(r.len(), 3);
        assert_eq!(r.values(), vec![2.0, 3.0, 4.0]);
        assert_eq!(r.latest().unwrap().value, 4.0);
    }

    #[test]
    fn test_mean_variance_std_dev() {
        let mut r = RollingTimeSeries::new(5);
        r.push(1, 10.0);
        r.push(2, 20.0);
        r.push(3, 30.0);
        r.push(4, 40.0);
        r.push(5, 50.0);

        assert!((r.mean() - 30.0).abs() < 1e-9);
        assert!((r.variance() - 250.0).abs() < 1e-9);
        assert!((r.std_dev() - 15.8113883).abs() < 1e-4);
        assert_eq!(r.min(), 10.0);
        assert_eq!(r.max(), 50.0);
    }

    #[test]
    fn test_z_score_anomaly_detection() {
        let mut r = RollingTimeSeries::new(10);
        for t in 1..=8 {
            r.push(t, 20.0);
        }
        r.push(9, 21.0);
        r.push(10, 19.0);

        let spike_z = r.z_score(50.0);
        assert!(spike_z > 3.0, "Expected spike z-score > 3.0, got {}", spike_z);
    }

    #[test]
    fn test_ols_linear_regression() {
        let mut r = RollingTimeSeries::new(5);
        // y = 2x + 5
        r.push(0, 5.0);
        r.push(10, 25.0);
        r.push(20, 45.0);
        r.push(30, 65.0);

        let reg = r.linear_regression().expect("regression failed");
        assert!((reg.slope - 2.0).abs() < 1e-4, "slope was {}", reg.slope);
        assert!((reg.intercept - 5.0).abs() < 1e-4, "intercept was {}", reg.intercept);
        assert!((reg.r_squared - 1.0).abs() < 1e-4, "r2 was {}", reg.r_squared);
    }

    #[test]
    fn test_tte_prediction() {
        let mut r = RollingTimeSeries::new(10);
        // Growth at 10 units per second
        r.push(0, 100.0);
        r.push(10, 200.0);
        r.push(20, 300.0);
        r.push(30, 400.0);

        // Ceiling at 1000.0, current value is 400.0. Remaining: 600. At 10/s => 60s
        let tte = r.predict_time_to_limit(400.0, 1000.0);
        assert_eq!(tte, Some(60));
    }

    #[test]
    fn test_percentile_calculation() {
        let mut r = RollingTimeSeries::new(10);
        for i in 1..=10 {
            r.push(i, i as f64);
        }
        assert!((r.percentile(0.50) - 5.5).abs() < 1e-4);
        assert_eq!(r.percentile(0.0), 1.0);
        assert_eq!(r.percentile(1.0), 10.0);
    }

    #[test]
    fn test_ema_computation() {
        let mut r = RollingTimeSeries::new(4);
        r.push(1, 10.0);
        r.push(2, 20.0);
        r.push(3, 30.0);

        let ema = r.exponential_moving_average(0.5);
        // ema(0) = 10
        // ema(1) = 0.5*20 + 0.5*10 = 15
        // ema(2) = 0.5*30 + 0.5*15 = 22.5
        assert!((ema - 22.5).abs() < 1e-4);
    }

    #[test]
    fn test_sawtooth_pattern_detection() {
        let mut r = RollingTimeSeries::new(20);
        // Cycle 1: rise from 50 to 100 then drop to 60
        r.push(1, 50.0);
        r.push(2, 75.0);
        r.push(3, 100.0);
        r.push(4, 60.0); // drop 40%

        // Cycle 2: rise from 60 to 120 then drop to 75
        r.push(5, 80.0);
        r.push(6, 120.0);
        r.push(7, 75.0); // drop 37.5%

        // Cycle 3: rise from 75 to 140
        r.push(8, 110.0);
        r.push(9, 140.0);

        let st = r.detect_sawtooth_pattern();
        assert!(st.is_some());
        let m = st.unwrap();
        assert!(m.drops_count >= 2);
        assert!(m.floor_climb_rate > 0.0);
    }
}
