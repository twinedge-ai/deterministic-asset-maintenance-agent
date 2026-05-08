use asset_agent_core::schemas::ObservationSnapshot;
use chrono::{DateTime, Duration, FixedOffset};
use std::collections::VecDeque;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FrameBufferError {
    #[error("failed to parse timestamp {timestamp}: {source}")]
    Timestamp {
        timestamp: String,
        source: chrono::ParseError,
    },
}

#[derive(Debug, Clone)]
pub struct FrameBuffer {
    max_len: usize,
    frames: VecDeque<ObservationSnapshot>,
}

impl FrameBuffer {
    pub fn new(max_len: usize) -> Self {
        Self {
            max_len,
            frames: VecDeque::with_capacity(max_len),
        }
    }

    pub fn push(&mut self, snapshot: ObservationSnapshot) {
        if self.frames.len() == self.max_len {
            self.frames.pop_front();
        }
        self.frames.push_back(snapshot);
    }

    pub fn latest(&self) -> Option<&ObservationSnapshot> {
        self.frames.back()
    }

    pub fn window_ending_at(
        &self,
        as_of_timestamp: &str,
        window_seconds: i64,
    ) -> Result<Vec<ObservationSnapshot>, FrameBufferError> {
        let as_of = parse_ts(as_of_timestamp)?;
        let start = as_of - Duration::seconds(window_seconds);
        let mut selected = Vec::new();
        for frame in &self.frames {
            let ts = parse_ts(&frame.timestamp)?;
            if ts >= start && ts <= as_of {
                selected.push(frame.clone());
            }
        }
        Ok(selected)
    }

    pub fn from_snapshots(
        max_len: usize,
        snapshots: impl IntoIterator<Item = ObservationSnapshot>,
    ) -> Self {
        let mut buffer = Self::new(max_len);
        for snapshot in snapshots {
            buffer.push(snapshot);
        }
        buffer
    }
}

fn parse_ts(timestamp: &str) -> Result<DateTime<FixedOffset>, FrameBufferError> {
    DateTime::parse_from_rfc3339(timestamp).map_err(|source| FrameBufferError::Timestamp {
        timestamp: timestamp.to_string(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::FrameBuffer;
    use asset_agent_core::replay::read_trace;
    use std::path::Path;

    #[test]
    fn selects_replay_window() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let snapshots = read_trace(root.join("traces/hp_pump_1_low_suction_pressure.jsonl"))
            .expect("trace should load");
        let buffer = FrameBuffer::from_snapshots(100, snapshots);
        let window = buffer
            .window_ending_at("2026-05-07T00:01:00Z", 10)
            .expect("window should select");
        assert_eq!(window.len(), 1);
        assert_eq!(window[0].asset_id, "hp_pump_1");
    }
}
