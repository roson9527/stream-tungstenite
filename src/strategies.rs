use rand::{rngs::StdRng, Rng, SeedableRng};
use std::time::Duration;

/// Defines the exponential backoff strategy for retry operations.
#[derive(Clone)]
pub struct ExpBackoffStrategy {
    min: Duration,
    max: Option<Duration>,
    factor: f64,
    jitter: f64,
    seed: Option<u64>,
}

impl ExpBackoffStrategy {
    /// Creates a new `ExpBackoffStrategy` with specified minimum duration, growth factor, and jitter.
    pub fn new(min: Duration, factor: f64, jitter: f64) -> Self {
        Self {
            min,
            max: None,
            factor,
            jitter,
            seed: None,
        }
    }

    /// Sets the maximum wait duration for the strategy.
    pub fn with_max(mut self, max: Duration) -> Self {
        self.max = Some(max);
        self
    }

    /// Sets the seed for random jitter generation.
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }
}

impl Default for ExpBackoffStrategy {
    fn default() -> Self {
        Self {
            min: Duration::from_secs(1),
            max: Some(Duration::from_secs(60)),
            factor: 2.0,
            jitter: 0.05,
            seed: None,
        }
    }
}

impl IntoIterator for ExpBackoffStrategy {
    type Item = Duration;
    type IntoIter = ExpBackoffIter;

    fn into_iter(self) -> Self::IntoIter {
        let init = self.min.as_secs_f64();
        let rng = match self.seed {
            Some(seed) => StdRng::seed_from_u64(seed),
            None => StdRng::from_os_rng(),
        };

        ExpBackoffIter {
            strategy: self,
            init,
            pow: 0,
            rng,
        }
    }
}

/// Iterator for generating exponential backoff durations.
pub struct ExpBackoffIter {
    strategy: ExpBackoffStrategy,
    init: f64,
    pow: u32,
    rng: StdRng,
}

pub trait ResetIterExt {
    fn reset(&mut self);
}

impl ResetIterExt for ExpBackoffIter {
    /// Resets the iterator to its initial state.
    fn reset(&mut self) {
        self.pow = 0;
    }
}

impl Iterator for ExpBackoffIter {
    type Item = Duration;

    fn next(&mut self) -> Option<Self::Item> {
        // Check for overflow before computing expensive operations
        let power_val = self.strategy.factor.powf(self.pow as f64);
        
        // Guard against overflow at the powf stage
        if !power_val.is_finite() || power_val > 1e10 {
            // If we've overflowed, return the max duration
            let result = self.strategy.max.unwrap_or(Duration::from_secs(60));
            self.pow += 1;
            return Some(result);
        }
        
        let base = self.init * power_val;
        
        // Additional check after multiplication
        if !base.is_finite() || base > 1e10 {
            let result = self.strategy.max.unwrap_or(Duration::from_secs(60));
            self.pow += 1;
            return Some(result);
        }
        
        let jitter = base * self.strategy.jitter * (self.rng.random::<f64>() * 2. - 1.);
        let total_secs = base + jitter;
        
        // Guard against overflow and invalid values
        let current = if total_secs.is_finite() && total_secs >= 0.0 && total_secs <= 1e9 {
            // Try to create Duration, with fallback on error
            Duration::from_secs_f64(total_secs.min(1e9))
        } else {
            // Fallback to max or reasonable default for invalid values
            self.strategy.max.unwrap_or(Duration::from_secs(60))
        };
        
        self.pow += 1;

        Some(match self.strategy.max {
            Some(max) => max.min(current),
            None => current,
        })
    }
}

pub trait DurationIteratorExt: Iterator<Item = Duration> + ResetIterExt + Send + Sync {}
pub type DurationIterator = Box<dyn DurationIteratorExt>;

impl DurationIteratorExt for ExpBackoffIter {}

#[cfg(test)]
mod test {
    use crate::strategies::{ExpBackoffStrategy, ResetIterExt};
    use std::time::Duration;

    #[tokio::test]
    async fn test_reset() {
        let strategy = ExpBackoffStrategy::default();
        let mut iter = strategy.into_iter();

        // Simulate some retry operations
        for _ in 0..3 {
            println!("{:?}", iter.next());
        }

        // Reset the iterator
        iter.reset();

        // Retry operations again from the start
        for _ in 0..3 {
            println!("{:?}", iter.next());
        }
    }
    
    #[test]
    fn test_default_strategy() {
        let strategy = ExpBackoffStrategy::default();
        let mut iter = strategy.into_iter();
        
        // First delay should be between 0.95 - 1.05 seconds (1 second ± 5%)
        let first = iter.next().unwrap();
        assert!(first >= Duration::from_millis(950));
        assert!(first <= Duration::from_millis(1050));
        
        // Second delay should be between 1.9 - 2.1 seconds (2 seconds ± 5%)
        let second = iter.next().unwrap();
        assert!(second >= Duration::from_millis(1900));
        assert!(second <= Duration::from_millis(2100));
        
        // Verify it doesn't exceed the maximum value
        for _ in 0..10 {
            let delay = iter.next().unwrap();
            assert!(delay <= Duration::from_secs(60));
        }
    }
}
