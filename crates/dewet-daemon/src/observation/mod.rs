use std::collections::VecDeque;

use chrono::{DateTime, Utc};

use crate::{bridge::ChatPacket, config::ObservationConfig, vision::VisionFrame};

pub struct ObservationBuffer {
    config: ObservationConfig,
    screen_history: VecDeque<ScreenSummary>,
    chat_history: VecDeque<ChatPacket>,
    last_user_message: Option<DateTime<Utc>>,
}

impl ObservationBuffer {
    pub fn new(config: ObservationConfig) -> Self {
        Self {
            config,
            screen_history: VecDeque::new(),
            chat_history: VecDeque::new(),
            last_user_message: None,
        }
    }

    pub fn ingest_screen(&mut self, frame: VisionFrame) -> Observation {
        let summary = ScreenSummary::from_frame(&frame);
        self.screen_history.push_back(summary.clone());
        while self.screen_history.len() > self.config.screen_history {
            self.screen_history.pop_front();
        }

        Observation {
            frame,
            screen_summary: summary,
            recent_chat: self.chat_history.iter().cloned().collect(),
            seconds_since_user_message: self
                .last_user_message
                .map(|ts| (Utc::now() - ts).num_seconds().max(0) as u64)
                .unwrap_or(u64::MAX),
        }
    }

    pub fn record_chat(&mut self, packet: ChatPacket) {
        if packet.sender == "user" {
            self.last_user_message =
                DateTime::<Utc>::from_timestamp(packet.timestamp, 0).or_else(|| Some(Utc::now()));
        }
        self.chat_history.push_back(packet);
        while self.chat_history.len() > self.config.chat_depth {
            self.chat_history.pop_front();
        }
    }
}

#[derive(Clone)]
pub struct ScreenSummary {
    pub timestamp: DateTime<Utc>,
    pub diff_score: f32,
    pub notes: String,
}

impl ScreenSummary {
    fn from_frame(frame: &VisionFrame) -> Self {
        let mut notes = format!(
            "diff={:.4}, dims={}x{}",
            frame.diff_score,
            frame.image.width(),
            frame.image.height()
        );
        if frame.diff_score < 0.02 {
            notes.push_str(" • stable view");
        }
        Self {
            timestamp: frame.timestamp,
            diff_score: frame.diff_score,
            notes,
        }
    }
}

pub struct Observation {
    pub frame: VisionFrame,
    pub screen_summary: ScreenSummary,
    pub recent_chat: Vec<ChatPacket>,
    pub seconds_since_user_message: u64,
}
