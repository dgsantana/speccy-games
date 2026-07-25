//! Score keeping, including the extra life every 10000 points.

/// The current and best scores.
#[derive(Debug, Clone, Copy, Default)]
pub struct Score {
    pub current: u32,
    pub high: u32,
    awarded: u32,
}

impl Score {
    /// Add to the score. Returns true when this crossed a 10000 boundary and
    /// earned an extra life.
    pub fn add(&mut self, amount: u32) -> bool {
        self.current = (self.current + amount).min(999_999);
        if self.current / 10_000 > self.awarded {
            self.awarded += 1;
            true
        } else {
            false
        }
    }

    /// Start a new game, keeping the high score.
    pub fn reset(&mut self) {
        self.high = self.high.max(self.current);
        self.current = 0;
        self.awarded = 0;
    }

    /// Fold the current score into the high score.
    pub fn record_high(&mut self) {
        self.high = self.high.max(self.current);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_extra_life_is_awarded_once_per_ten_thousand() {
        let mut score = Score::default();
        assert!(!score.add(9_999));
        assert!(score.add(1));
        assert!(!score.add(9_999));
        assert!(score.add(1));
        assert_eq!(score.current, 20_000);
    }

    #[test]
    fn resetting_keeps_the_best_score() {
        let mut score = Score::default();
        score.add(1234);
        score.reset();
        assert_eq!(score.high, 1234);
        assert_eq!(score.current, 0);
    }
}
