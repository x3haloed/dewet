use std::collections::VecDeque;

use chrono::{DateTime, Utc};
use image::RgbaImage;

use crate::{bridge::{ChatPacket, MemoryTier}, config::ObservationConfig, vision::VisionFrame};

/// Stores a screenshot that resulted in an approved response
#[derive(Clone)]
pub struct ApprovedScreenshot {
    pub image: RgbaImage,
    pub timestamp: DateTime<Utc>,
}

pub struct ObservationBuffer {
    config: ObservationConfig,
    screen_history: VecDeque<ScreenSummary>,
    chat_history: VecDeque<ChatPacket>,
    last_user_message: Option<DateTime<Utc>>,
    /// Screenshots that resulted in approved (Speak) responses
    approved_screenshots: VecDeque<ApprovedScreenshot>,
    /// User messages that arrived since last perception tick (to be batched)
    pending_user_messages: Vec<ChatPacket>,
}

impl ObservationBuffer {
    pub fn new(config: ObservationConfig) -> Self {
        Self {
            config,
            screen_history: VecDeque::new(),
            chat_history: VecDeque::new(),
            last_user_message: None,
            approved_screenshots: VecDeque::new(),
            pending_user_messages: Vec::new(),
        }
    }
    
    /// Record a screenshot that resulted in an approved response
    pub fn record_approved_screenshot(&mut self, image: RgbaImage) {
        self.approved_screenshots.push_back(ApprovedScreenshot {
            image,
            timestamp: Utc::now(),
        });
        // Keep only the last 3 approved screenshots
        while self.approved_screenshots.len() > 3 {
            self.approved_screenshots.pop_front();
        }
    }
    
    /// Get recent approved screenshots for visual history
    pub fn approved_screenshots(&self) -> Vec<&ApprovedScreenshot> {
        self.approved_screenshots.iter().collect()
    }
    
    /// Queue a user message to be processed in the next perception tick
    pub fn queue_user_message(&mut self, packet: ChatPacket) {
        self.pending_user_messages.push(packet);
    }
    
    /// Drain pending user messages and add them to chat history
    /// Returns the messages that were processed (for logging/display)
    pub fn flush_pending_messages(&mut self) -> Vec<ChatPacket> {
        let messages = std::mem::take(&mut self.pending_user_messages);
        for packet in &messages {
            // Update last user message timestamp
            self.last_user_message =
                DateTime::<Utc>::from_timestamp(packet.timestamp, 0).or_else(|| Some(Utc::now()));
            // Add to chat history
            self.chat_history.push_back(packet.clone());
            while self.chat_history.len() > self.config.chat_depth {
                self.chat_history.pop_front();
            }
        }
        messages
    }
    
    /// Check if there are pending user messages
    pub fn has_pending_messages(&self) -> bool {
        !self.pending_user_messages.is_empty()
    }

    pub fn ingest_screen(
        &mut self,
        frame: VisionFrame,
        composite: Option<RgbaImage>,
        ariaos: Option<RgbaImage>,
    ) -> Observation {
        let summary = ScreenSummary::from_frame(&frame);
        self.screen_history.push_back(summary.clone());
        while self.screen_history.len() > self.config.screen_history {
            self.screen_history.pop_front();
        }

        // Use VLM-filtered chat (hot + warm only, limited count)
        let filtered_chat = self.vlm_filtered_chat();
        
        Observation {
            frame,
            composite,
            ariaos,
            screen_summary: summary,
            recent_chat: filtered_chat,
            all_chat: self.chat_history.iter().cloned().collect(),
            seconds_since_user_message: self
                .last_user_message
                .map(|ts| (Utc::now() - ts).num_seconds().max(0) as u64)
                .unwrap_or(u64::MAX),
        }
    }

    /// Record a chat message directly (for assistant messages or loading from DB)
    /// For user messages during runtime, use queue_user_message instead
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
    
    pub fn chat_count(&self) -> usize {
        self.chat_history.len()
    }
    
    pub fn pending_message_count(&self) -> usize {
        self.pending_user_messages.len()
    }
    
    /// Apply time-based decay to all chat messages and update their tiers
    /// Call this at the start of each perception tick
    pub fn apply_relevance_decay(&mut self, minutes_since_last: f32) {
        let decay_rate = self.config.decay_rate;
        let forget_threshold = self.config.forget_threshold;
        
        for packet in self.chat_history.iter_mut() {
            packet.apply_decay(decay_rate, minutes_since_last);
            packet.update_tier(forget_threshold);
        }
    }
    
    /// Get messages filtered by tier for VLM context
    /// Returns only hot and warm messages, limited to max_vlm_messages
    pub fn vlm_filtered_chat(&self) -> Vec<ChatPacket> {
        let max = self.config.max_vlm_messages;
        
        // Prioritize hot messages, then warm, skip cold
        let mut messages: Vec<_> = self.chat_history
            .iter()
            .filter(|p| p.tier != MemoryTier::Cold)
            .cloned()
            .collect();
        
        // Sort by relevance (highest first), then by timestamp (newest first) as tiebreaker
        messages.sort_by(|a, b| {
            b.relevance.partial_cmp(&a.relevance)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.timestamp.cmp(&a.timestamp))
        });
        
        // Take only the most relevant messages
        messages.truncate(max);
        
        // Re-sort by timestamp for chronological order in context
        messages.sort_by_key(|p| p.timestamp);
        
        messages
    }
    
    /// Boost relevance of a message (e.g., when it triggers a response)
    pub fn boost_relevance(&mut self, timestamp: i64, boost: f32) {
        for packet in self.chat_history.iter_mut() {
            if packet.timestamp == timestamp {
                packet.relevance = (packet.relevance + boost).min(1.0);
                packet.update_tier(self.config.forget_threshold);
                break;
            }
        }
    }
    
    /// Get tier distribution for debugging
    pub fn tier_stats(&self) -> (usize, usize, usize) {
        let mut hot = 0;
        let mut warm = 0;
        let mut cold = 0;
        for p in &self.chat_history {
            match p.tier {
                MemoryTier::Hot => hot += 1,
                MemoryTier::Warm => warm += 1,
                MemoryTier::Cold => cold += 1,
            }
        }
        (hot, warm, cold)
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
    pub composite: Option<RgbaImage>,
    /// ARIAOS rendered image (companion's self-managed display)
    pub ariaos: Option<RgbaImage>,
    pub screen_summary: ScreenSummary,
    /// Filtered chat for VLM (hot + warm only, limited)
    pub recent_chat: Vec<ChatPacket>,
    /// Full chat history for rendering (includes cold)
    pub all_chat: Vec<ChatPacket>,
    pub seconds_since_user_message: u64,
}
