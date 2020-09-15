use crate::r#type::AsRelativeTime;

#[derive(Debug, Clone)]
pub struct TimeRange {
    start: chrono::Duration,
    end: chrono::Duration,
}

impl TimeRange {
    pub fn new(start: chrono::Duration, length: chrono::Duration) -> Self {
        let end = start + length;

        Self::with_bounds(start, end)
    }

    pub fn with_bounds(start: chrono::Duration, end: chrono::Duration) -> Self {
        assert!(start < end);

        Self { start, end }
    }

    pub fn with_default_len(start: chrono::Duration) -> Self {
        let default_len = chrono::Duration::seconds(10);

        Self::new(start, default_len)
    }

    pub fn start(&self) -> chrono::Duration {
        self.start
    }

    pub fn end(&self) -> chrono::Duration {
        self.end
    }

    pub fn length(&self) -> chrono::Duration {
        self.end - self.start
    }

    pub fn offset(&self, vtime: chrono::Duration) -> chrono::Duration {
        vtime - self.start
    }

    pub fn ratio(&self, vtime: chrono::Duration) -> f32 {
        self.offset(vtime).as_relative_time() / self.length().as_relative_time()
    }

    pub fn contains(&self, vtime: chrono::Duration) -> bool {
        self.start <= vtime && vtime <= self.end
    }
}

impl Default for TimeRange {
    fn default() -> Self {
        Self::with_default_len(chrono::Duration::zero())
    }
}
