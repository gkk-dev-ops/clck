use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
pub struct Countdown {
    duration: Duration,
    accumulated: Duration,
    resumed_at: Option<Instant>,
}

impl Countdown {
    pub fn new(started_at: Instant, duration: Duration) -> Self {
        Self {
            duration,
            accumulated: Duration::ZERO,
            resumed_at: Some(started_at),
        }
    }

    fn elapsed(&self, now: Instant) -> Duration {
        self.accumulated
            + self
                .resumed_at
                .map_or(Duration::ZERO, |t| now.duration_since(t))
    }

    pub fn remaining(&self, now: Instant) -> Duration {
        self.duration.saturating_sub(self.elapsed(now))
    }

    pub fn is_finished(&self, now: Instant) -> bool {
        self.remaining(now).is_zero()
    }

    pub fn pause(&mut self, now: Instant) {
        if let Some(resumed_at) = self.resumed_at.take() {
            self.accumulated += now.duration_since(resumed_at);
        }
    }

    pub fn resume(&mut self, now: Instant) {
        if self.resumed_at.is_none() {
            self.resumed_at = Some(now);
        }
    }

    pub fn is_paused(&self) -> bool {
        self.resumed_at.is_none()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Stopwatch {
    accumulated: Duration,
    resumed_at: Option<Instant>,
}

impl Stopwatch {
    pub fn new(started_at: Instant) -> Self {
        Self {
            accumulated: Duration::ZERO,
            resumed_at: Some(started_at),
        }
    }

    pub fn elapsed(&self, now: Instant) -> Duration {
        self.accumulated
            + self
                .resumed_at
                .map_or(Duration::ZERO, |t| now.duration_since(t))
    }

    pub fn pause(&mut self, now: Instant) {
        if let Some(resumed_at) = self.resumed_at.take() {
            self.accumulated += now.duration_since(resumed_at);
        }
    }

    pub fn resume(&mut self, now: Instant) {
        if self.resumed_at.is_none() {
            self.resumed_at = Some(now);
        }
    }

    pub fn is_paused(&self) -> bool {
        self.resumed_at.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::{Countdown, Stopwatch};
    use std::time::{Duration, Instant};

    #[test]
    fn remaining_time_never_underflows() {
        let start = Instant::now();
        let timer = Countdown::new(start, Duration::from_secs(90));
        assert_eq!(
            timer.remaining(start + Duration::from_secs(30)),
            Duration::from_secs(60)
        );
        assert_eq!(
            timer.remaining(start + Duration::from_secs(100)),
            Duration::ZERO
        );
    }

    #[test]
    fn pause_freezes_remaining_time() {
        let start = Instant::now();
        let mut timer = Countdown::new(start, Duration::from_secs(60));
        let paused_at = start + Duration::from_secs(10);
        timer.pause(paused_at);
        assert!(timer.is_paused());
        // Time should not advance while paused
        assert_eq!(
            timer.remaining(paused_at + Duration::from_secs(20)),
            Duration::from_secs(50)
        );
    }

    #[test]
    fn resume_continues_from_paused_remaining() {
        let start = Instant::now();
        let mut timer = Countdown::new(start, Duration::from_secs(60));
        timer.pause(start + Duration::from_secs(10));
        let resumed_at = start + Duration::from_secs(30);
        timer.resume(resumed_at);
        assert!(!timer.is_paused());
        // After resuming, 5 more seconds pass → remaining = 60 - 10 - 5 = 45
        assert_eq!(
            timer.remaining(resumed_at + Duration::from_secs(5)),
            Duration::from_secs(45)
        );
    }

    #[test]
    fn stopwatch_counts_up() {
        let start = Instant::now();
        let watch = Stopwatch::new(start);
        assert_eq!(watch.elapsed(start + Duration::from_secs(30)), Duration::from_secs(30));
        assert_eq!(watch.elapsed(start + Duration::from_secs(90)), Duration::from_secs(90));
    }

    #[test]
    fn stopwatch_pause_freezes_elapsed() {
        let start = Instant::now();
        let mut watch = Stopwatch::new(start);
        watch.pause(start + Duration::from_secs(10));
        assert!(watch.is_paused());
        // Elapsed should not advance while paused
        assert_eq!(watch.elapsed(start + Duration::from_secs(30)), Duration::from_secs(10));
    }

    #[test]
    fn stopwatch_resume_continues_accumulating() {
        let start = Instant::now();
        let mut watch = Stopwatch::new(start);
        watch.pause(start + Duration::from_secs(10));
        let resumed_at = start + Duration::from_secs(20);
        watch.resume(resumed_at);
        assert!(!watch.is_paused());
        // 5 more seconds after resume → total elapsed = 10 + 5 = 15
        assert_eq!(watch.elapsed(resumed_at + Duration::from_secs(5)), Duration::from_secs(15));
    }
}
