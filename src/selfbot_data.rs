use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tokio::fs::{create_dir_all, OpenOptions};
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplyContext {
    pub message_id: String,
    pub author_id: String,
    pub author_name: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    pub id: String,
    pub author_id: String,
    pub author_name: String,
    pub content: String,
    pub channel_id: String,
    pub guild_id: Option<String>,
    pub timestamp: String,
    pub reply_to: Option<ReplyContext>,
    pub quality_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoreStats {
    pub total_seen: u64,
    pub saved: u64,
    pub skipped_bot: u64,
    pub skipped_empty: u64,
    pub skipped_duplicate_id: u64,
    pub skipped_duplicate_content: u64,
    pub skipped_low_quality: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessOutcome {
    Saved,
    SkippedBot,
    SkippedEmpty,
    SkippedDuplicateId,
    SkippedDuplicateContent,
    SkippedLowQuality,
}

pub struct MessageStore {
    data_dir: PathBuf,
    seen_ids: HashSet<String>,
    seen_content_hashes: HashSet<String>,
    stats: StoreStats,
}

impl MessageStore {
    pub async fn load(data_dir: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let data_dir = data_dir.as_ref().to_path_buf();
        create_dir_all(data_dir.join("clean")).await?;
        create_dir_all(data_dir.join("state")).await?;
        create_dir_all(data_dir.join("reports")).await?;

        let seen_ids = load_string_set(&data_dir.join("state/seen_ids.json")).await?;
        let seen_content_hashes =
            load_string_set(&data_dir.join("state/seen_content_hashes.json")).await?;
        let stats = load_stats(&data_dir.join("reports/latest.json")).await?;

        Ok(Self {
            data_dir,
            seen_ids,
            seen_content_hashes,
            stats,
        })
    }

    pub async fn process(
        &mut self,
        input: IncomingMessage,
    ) -> Result<ProcessOutcome, Box<dyn std::error::Error + Send + Sync>> {
        self.stats.total_seen += 1;

        if input.is_bot {
            self.stats.skipped_bot += 1;
            return Ok(ProcessOutcome::SkippedBot);
        }

        let content = clean_text(&input.content);
        if content.is_empty() {
            self.stats.skipped_empty += 1;
            return Ok(ProcessOutcome::SkippedEmpty);
        }

        if self.seen_ids.contains(&input.id) {
            self.stats.skipped_duplicate_id += 1;
            return Ok(ProcessOutcome::SkippedDuplicateId);
        }

        let content_hash = hash_content(&content);
        if self.seen_content_hashes.contains(&content_hash) {
            self.stats.skipped_duplicate_content += 1;
            self.seen_ids.insert(input.id);
            return Ok(ProcessOutcome::SkippedDuplicateContent);
        }

        let quality_score = quality_score(&content);
        if quality_score < 0.45 {
            self.stats.skipped_low_quality += 1;
            self.seen_ids.insert(input.id);
            return Ok(ProcessOutcome::SkippedLowQuality);
        }

        let reply_to = input.reply_to.map(|reply| ReplyContext {
            message_id: reply.message_id,
            author_id: reply.author_id,
            author_name: reply.author_name,
            content: clean_text(&reply.content),
        });

        let stored = StoredMessage {
            id: input.id.clone(),
            author_id: input.author_id,
            author_name: input.author_name,
            content,
            channel_id: input.channel_id,
            guild_id: input.guild_id,
            timestamp: input.timestamp,
            reply_to,
            quality_score,
        };

        self.append_message(&stored).await?;
        self.seen_ids.insert(input.id);
        self.seen_content_hashes.insert(content_hash);
        self.stats.saved += 1;
        self.persist_state().await?;

        Ok(ProcessOutcome::Saved)
    }

    pub fn stats(&self) -> &StoreStats {
        &self.stats
    }

    async fn append_message(
        &self,
        message: &StoredMessage,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let path = self.data_dir.join("clean/messages.jsonl");
        let line = serde_json::to_string(message)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?;
        file.write_all(format!("{line}\n").as_bytes()).await?;
        Ok(())
    }

    async fn persist_state(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        save_string_set(&self.data_dir.join("state/seen_ids.json"), &self.seen_ids).await?;
        save_string_set(
            &self.data_dir.join("state/seen_content_hashes.json"),
            &self.seen_content_hashes,
        )
        .await?;

        let report = serde_json::json!({
            "stats": self.stats,
            "unique_messages": self.seen_ids.len(),
            "unique_content": self.seen_content_hashes.len(),
        });

        tokio::fs::write(
            self.data_dir.join("reports/latest.json"),
            serde_json::to_string_pretty(&report)?,
        )
        .await?;

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct IncomingMessage {
    pub id: String,
    pub author_id: String,
    pub author_name: String,
    pub content: String,
    pub channel_id: String,
    pub guild_id: Option<String>,
    pub timestamp: String,
    pub is_bot: bool,
    pub reply_to: Option<IncomingReply>,
}

#[derive(Debug, Clone)]
pub struct IncomingReply {
    pub message_id: String,
    pub author_id: String,
    pub author_name: String,
    pub content: String,
}

pub fn clean_text(text: &str) -> String {
    text.chars()
        .filter(|character| {
            matches!(
                character,
                '\u{0600}'..='\u{06FF}'
                    | 'a'..='z'
                    | 'A'..='Z'
                    | '0'..='9'
                    | ' '
                    | '.'
                    | ','
                    | '!'
                    | '?'
                    | '\u{061F}'
                    | ':'
                    | ';'
                    | '-'
                    | '_'
            )
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn hash_content(content: &str) -> String {
    let normalized = content.to_lowercase();
    let digest = Sha256::digest(normalized.as_bytes());
    format!("{:x}", digest)
}

fn quality_score(content: &str) -> f32 {
    let char_count = content.chars().count();
    if char_count < 2 {
        return 0.0;
    }

    let mut score: f32 = 0.55;

    if char_count >= 4 {
        score += 0.15;
    }
    if char_count >= 10 {
        score += 0.1;
    }

    if has_excessive_repetition(content) {
        score -= 0.35;
    }

    if content.chars().all(|c| c.is_ascii_digit()) {
        score -= 0.4;
    }

    if content.split_whitespace().count() >= 2 {
        score += 0.1;
    }

    if contains_arabic(content) {
        score += 0.1;
    }

    score.clamp(0.0, 1.0)
}

fn has_excessive_repetition(content: &str) -> bool {
    let chars: Vec<char> = content.chars().collect();
    if chars.len() < 4 {
        return false;
    }

    let mut run = 1;
    let mut max_run = 1;

    for index in 1..chars.len() {
        if chars[index] == chars[index - 1] {
            run += 1;
            max_run = max_run.max(run);
        } else {
            run = 1;
        }
    }

    max_run >= 6 || (max_run as f32 / chars.len() as f32) > 0.7
}

fn contains_arabic(content: &str) -> bool {
    content.chars().any(|c| ('\u{0600}'..='\u{06FF}').contains(&c))
}

async fn load_string_set(path: &Path) -> Result<HashSet<String>, Box<dyn std::error::Error + Send + Sync>> {
    if !path.exists() {
        return Ok(HashSet::new());
    }

    let raw = tokio::fs::read_to_string(path).await?;
    let values: Vec<String> = serde_json::from_str(&raw)?;
    Ok(values.into_iter().collect())
}

async fn save_string_set(
    path: &Path,
    values: &HashSet<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut list: Vec<&String> = values.iter().collect();
    list.sort();
    tokio::fs::write(path, serde_json::to_string_pretty(&list)?).await?;
    Ok(())
}

async fn load_stats(path: &Path) -> Result<StoreStats, Box<dyn std::error::Error + Send + Sync>> {
    if !path.exists() {
        return Ok(StoreStats::default());
    }

    let raw = tokio::fs::read_to_string(path).await?;
    let value: serde_json::Value = serde_json::from_str(&raw)?;
    Ok(serde_json::from_value(value["stats"].clone()).unwrap_or_default())
}

pub fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        format!("{}...", text.chars().take(max_chars).collect::<String>())
    }
}
