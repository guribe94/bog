//! Gap Detection for Market Data Streams
//!
//! Detects sequence number gaps that indicate:
//! - Network packet loss
//! - Huginn restart (with epoch change)
//! - Shared memory buffer overflow
//!
//! Handles wraparound at u64::MAX for long-running sessions.

/// Detects gaps in sequence numbers with wraparound support
///
/// # Wraparound Arithmetic
///
/// For u64 sequences that wrap around at u64::MAX → 0:
/// - Normal case: gap = (next - last - 1) when next > last
/// - Wraparound case: gap = (next + u64::MAX - last) when next < last
///
/// # Example
///
/// ```ignore
/// let mut detector = GapDetector::new();
/// detector.check(1);      // OK, first message
/// detector.check(2);      // OK, no gap (2 - 1 - 1 = 0)
/// detector.check(5);      // Gap! (5 - 2 - 1 = 2, missing 3-4)
/// assert_eq!(detector.last_gap_size(), 2);
/// ```
#[derive(Debug, Clone, Default)]
pub struct GapDetector {
    /// Last sequence number processed
    last_sequence: u64,
    /// Size of last detected gap (0 if no gap)
    last_gap_size: u64,
    /// Whether a gap is currently detected
    gap_detected: bool,
    /// Whether detector is ready (has processed first message)
    ready: bool,
    /// Current Huginn epoch (for restart detection)
    last_epoch: u64,
}

impl GapDetector {
    /// Create a new GapDetector
    pub fn new() -> Self {
        Self {
            last_sequence: 0,
            last_gap_size: 0,
            gap_detected: false,
            ready: false,
            last_epoch: 0,
        }
    }

    /// Check a sequence number for gaps with wraparound support
    ///
    /// Returns the gap size if a gap was detected (0 if no gap)
    #[inline(always)]
    pub fn check(&mut self, current_sequence: u64) -> u64 {
        // First message: just record it, no gap possible
        if !self.ready {
            self.last_sequence = current_sequence;
            self.ready = true;
            self.gap_detected = false;
            self.last_gap_size = 0;
            return 0;
        }

        // Duplicate message: not a gap, return 0
        if current_sequence == self.last_sequence {
            return 0;
        }

        // Calculate gap with wraparound support
        let gap_size = self.calculate_gap(self.last_sequence, current_sequence);

        // Update state
        self.last_sequence = current_sequence;
        if gap_size > 0 {
            self.gap_detected = true;
            self.last_gap_size = gap_size;
        } else {
            self.gap_detected = false;
            self.last_gap_size = 0;
        }

        gap_size
    }

    /// Calculate gap between two sequence numbers with wraparound support
    ///
    /// # Wraparound Safe Comparison
    ///
    /// The key insight: if `current > last`, normal subtraction works.
    /// If `current < last`, then wraparound occurred.
    ///
    /// Formula:
    /// - if current > last: gap = current - last - 1
    /// - if current < last: gap = (u64::MAX - last) + current + 1 - 1
    ///                           = u64::MAX - last + current
    #[inline(always)]
    fn calculate_gap(&self, last: u64, current: u64) -> u64 {
        if current > last {
            // Normal case: no wraparound
            current - last - 1
        } else if current < last {
            // Wraparound occurred: u64::MAX → 0 → current
            // Gap = messages from (last+1) to u64::MAX + messages from 0 to (current-1)
            // = (u64::MAX - last) + current
            u64::MAX - last + current
        } else {
            // current == last: duplicate or reprocessing
            0
        }
    }

    /// Check for Huginn restart by comparing sequence and epoch
    ///
    /// Restart is detected when:
    /// - Sequence drops unexpectedly (next < last)
    /// - Epoch increases (last_epoch < new_epoch)
    #[inline]
    pub fn detect_restart(&mut self, sequence: u64, epoch: u64) -> bool {
        let is_restart = sequence < self.last_sequence && epoch > self.last_epoch;
        if is_restart {
            self.last_epoch = epoch;
        }
        is_restart
    }

    /// Update epoch for restart detection
    #[inline]
    pub fn set_epoch(&mut self, epoch: u64) {
        self.last_epoch = epoch;
    }

    /// Get last detected gap size
    #[inline]
    pub fn last_gap_size(&self) -> u64 {
        self.last_gap_size
    }

    /// Check if gap is currently detected
    #[inline]
    pub fn gap_detected(&self) -> bool {
        self.gap_detected
    }

    /// Check if detector is ready (has seen first message)
    #[inline]
    pub fn is_ready(&self) -> bool {
        self.ready
    }

    /// Get last sequence number
    #[inline]
    pub fn last_sequence(&self) -> u64 {
        self.last_sequence
    }

    /// Reset detector for recovery
    ///
    /// Called after snapshot recovery to clear gap state
    #[inline]
    pub fn reset(&mut self) {
        self.last_sequence = 0;
        self.last_gap_size = 0;
        self.gap_detected = false;
        self.ready = false;
    }

    /// Reset with known sequence (for snapshot recovery)
    ///
    /// After receiving a snapshot at sequence N, set detector to continue from N
    #[inline]
    pub fn reset_at_sequence(&mut self, sequence: u64) {
        self.last_sequence = sequence;
        self.last_gap_size = 0;
        self.gap_detected = false;
        self.ready = true; // We now have a known sequence
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normal_sequence() {
        let mut detector = GapDetector::new();

        assert_eq!(detector.check(1), 0); // First message
        assert_eq!(detector.check(2), 0); // No gap
        assert_eq!(detector.check(3), 0); // No gap
        assert!(!detector.gap_detected());
    }

    #[test]
    fn test_small_gap() {
        let mut detector = GapDetector::new();

        detector.check(1);
        detector.check(2);
        let gap = detector.check(5); // Gap: 5 - 2 - 1 = 2 (missing 3, 4)

        assert_eq!(gap, 2);
        assert!(detector.gap_detected());
        assert_eq!(detector.last_gap_size(), 2);
    }

    #[test]
    fn test_duplicate_not_gap() {
        let mut detector = GapDetector::new();

        detector.check(100);
        let gap = detector.check(100); // Duplicate

        assert_eq!(gap, 0);
        assert!(!detector.gap_detected());
    }

    #[test]
    fn test_wraparound() {
        let mut detector = GapDetector::new();

        detector.check(u64::MAX);
        let gap = detector.check(0); // Wraparound, no gap

        assert_eq!(gap, 0);
        assert!(!detector.gap_detected());
    }

    #[test]
    fn test_wraparound_with_gap() {
        let mut detector = GapDetector::new();

        detector.check(u64::MAX - 2);
        let gap = detector.check(5); // Gap across wraparound
                                      // Gap = (u64::MAX - (u64::MAX - 2)) + 5 = 2 + 5 = 7
                                      // Messages: u64::MAX-1, u64::MAX, 0, 1, 2, 3, 4 = 7 messages

        assert_eq!(gap, 7);
        assert!(detector.gap_detected());
    }

    #[test]
    fn test_large_gap() {
        let mut detector = GapDetector::new();

        detector.check(100);
        let gap = detector.check(1200); // Gap of 1099 messages

        assert_eq!(gap, 1099);
        assert!(detector.gap_detected());
    }

    #[test]
    fn test_reset() {
        let mut detector = GapDetector::new();

        detector.check(100);
        detector.check(105); // Gap

        assert!(detector.gap_detected());

        detector.reset();

        assert!(!detector.gap_detected());
        assert!(!detector.is_ready());
    }

    #[test]
    fn test_reset_at_sequence() {
        let mut detector = GapDetector::new();

        detector.check(100);
        detector.check(105); // Gap

        // After snapshot recovery at sequence 105
        detector.reset_at_sequence(105);

        assert!(detector.is_ready());
        assert!(!detector.gap_detected());
        assert_eq!(detector.last_sequence(), 105);

        // Continue normal operation
        assert_eq!(detector.check(106), 0); // No gap
    }

    #[test]
    fn test_epoch_restart_detection() {
        let mut detector = GapDetector::new();

        detector.check(1000);
        detector.set_epoch(1);

        // Sequence drops but epoch increases: restart
        let is_restart = detector.detect_restart(10, 2);

        assert!(is_restart);
        assert_eq!(detector.last_epoch, 2);
    }

    #[test]
    fn test_sequence_reset_without_epoch_change() {
        let mut detector = GapDetector::new();

        detector.check(1000);
        detector.set_epoch(1);

        // Sequence drops but epoch doesn't change: gap, not restart
        let is_restart = detector.detect_restart(10, 1);

        assert!(!is_restart);
    }
}
