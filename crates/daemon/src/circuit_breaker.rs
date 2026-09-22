use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitState {
    Closed,   // Normal operating state
    HalfOpen, // In backoff evaluation state
    Open,     // Tripped, auto-restart suspended
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashRecord {
    pub timestamp: DateTime<Utc>,
    pub exit_code: i32,
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub max_crashes_in_window: usize,
    pub window_duration: Duration,
    pub backoff_base_secs: u64,
    pub backoff_max_secs: u64,
    pub recovery_healthy_duration: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            max_crashes_in_window: 3,
            window_duration: Duration::from_secs(60),
            backoff_base_secs: 2,
            backoff_max_secs: 60,
            recovery_healthy_duration: Duration::from_secs(300),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CircuitDecision {
    Backoff(Duration),
    Trip { crashes: usize, window_secs: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerInfo {
    pub server_name: String,
    pub path: PathBuf,
    pub state: String,
    pub consecutive_crashes: usize,
    pub last_tripped: Option<String>,
}

#[derive(Debug)]
pub struct CrashCircuitBreaker {
    pub server_path: PathBuf,
    pub state: CircuitState,
    pub history: VecDeque<CrashRecord>,
    pub consecutive_crashes: usize,
    pub config: CircuitBreakerConfig,
    pub last_started: Option<DateTime<Utc>>,
    pub last_tripped: Option<DateTime<Utc>>,
}

impl CrashCircuitBreaker {
    pub fn new(server_path: PathBuf) -> Self {
        Self::with_config(server_path, CircuitBreakerConfig::default())
    }

    pub fn with_config(server_path: PathBuf, config: CircuitBreakerConfig) -> Self {
        Self {
            server_path,
            state: CircuitState::Closed,
            history: VecDeque::new(),
            consecutive_crashes: 0,
            config,
            last_started: None,
            last_tripped: None,
        }
    }

    pub fn record_start(&mut self) {
        self.last_started = Some(Utc::now());
    }

    pub fn check_recovery(&mut self) {
        if let Some(started) = self.last_started {
            let elapsed = Utc::now().signed_duration_since(started);
            if elapsed.to_std().unwrap_or_default() >= self.config.recovery_healthy_duration {
                // Server has run healthy for longer than recovery duration
                self.consecutive_crashes = 0;
                self.history.clear();
                if self.state == CircuitState::HalfOpen {
                    self.state = CircuitState::Closed;
                }
            }
        }
    }

    pub fn record_crash(&mut self, exit_code: i32) -> CircuitDecision {
        let now = Utc::now();
        self.check_recovery();

        // Prune older crash records outside the sliding window
        let window_chrono = chrono::Duration::from_std(self.config.window_duration)
            .unwrap_or_else(|_| chrono::Duration::seconds(60));
        while let Some(front) = self.history.front() {
            if now.signed_duration_since(front.timestamp) > window_chrono {
                self.history.pop_front();
            } else {
                break;
            }
        }

        // Record this crash
        self.history.push_back(CrashRecord {
            timestamp: now,
            exit_code,
        });
        self.consecutive_crashes += 1;

        // Evaluate if tripped
        if self.history.len() >= self.config.max_crashes_in_window {
            self.state = CircuitState::Open;
            self.last_tripped = Some(now);
            CircuitDecision::Trip {
                crashes: self.history.len(),
                window_secs: self.config.window_duration.as_secs(),
            }
        } else {
            self.state = CircuitState::HalfOpen;
            // Exponential backoff: base * 2^(consecutive - 1)
            let exponent = (self.consecutive_crashes.saturating_sub(1) as u32).min(10);
            let multiplier = 2u64.saturating_pow(exponent);
            let backoff_secs = (self.config.backoff_base_secs.saturating_mul(multiplier))
                .min(self.config.backoff_max_secs);
            CircuitDecision::Backoff(Duration::from_secs(backoff_secs))
        }
    }

    pub fn reset(&mut self) {
        self.state = CircuitState::Closed;
        self.history.clear();
        self.consecutive_crashes = 0;
        self.last_tripped = None;
    }

    pub fn is_tripped(&self) -> bool {
        self.state == CircuitState::Open
    }

    pub fn status_info(&self, server_name: &str) -> CircuitBreakerInfo {
        let state_str = match self.state {
            CircuitState::Closed => "Closed (Healthy)".to_string(),
            CircuitState::HalfOpen => format!(
                "HalfOpen (Backoff #{} - {} crashes in window)",
                self.consecutive_crashes,
                self.history.len()
            ),
            CircuitState::Open => format!(
                "Open (TRIPPED - {} crashes in window)",
                self.history.len()
            ),
        };

        let last_tripped_str = self
            .last_tripped
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string());

        CircuitBreakerInfo {
            server_name: server_name.to_string(),
            path: self.server_path.clone(),
            state: state_str,
            consecutive_crashes: self.consecutive_crashes,
            last_tripped: last_tripped_str,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exponential_backoff_and_tripping() {
        let path = PathBuf::from("/tmp/test-server");
        let config = CircuitBreakerConfig {
            max_crashes_in_window: 3,
            window_duration: Duration::from_secs(60),
            backoff_base_secs: 2,
            backoff_max_secs: 30,
            recovery_healthy_duration: Duration::from_secs(300),
        };
        let mut cb = CrashCircuitBreaker::with_config(path, config);

        assert_eq!(cb.state, CircuitState::Closed);
        assert!(!cb.is_tripped());

        // First crash -> backoff 2s (2 * 2^0)
        let dec1 = cb.record_crash(1);
        assert_eq!(dec1, CircuitDecision::Backoff(Duration::from_secs(2)));
        assert_eq!(cb.state, CircuitState::HalfOpen);
        assert!(!cb.is_tripped());

        // Second crash -> backoff 4s (2 * 2^1)
        let dec2 = cb.record_crash(1);
        assert_eq!(dec2, CircuitDecision::Backoff(Duration::from_secs(4)));
        assert_eq!(cb.state, CircuitState::HalfOpen);
        assert!(!cb.is_tripped());

        // Third crash within 60s -> TRIPPED
        let dec3 = cb.record_crash(1);
        assert_eq!(
            dec3,
            CircuitDecision::Trip {
                crashes: 3,
                window_secs: 60
            }
        );
        assert_eq!(cb.state, CircuitState::Open);
        assert!(cb.is_tripped());

        // Reset
        cb.reset();
        assert_eq!(cb.state, CircuitState::Closed);
        assert!(!cb.is_tripped());
        assert_eq!(cb.consecutive_crashes, 0);
    }
}
