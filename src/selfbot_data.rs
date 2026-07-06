use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tokio::fs::{create_dir_all, OpenOptions};
use tokio::io::AsyncWriteExt;

const MIN_QUALITY: f32 = 0.68;
const MIN_TRAINING_QUALITY: f32 = 0.75;
const MIN_CHARS: usize = 6;
const MIN_TRAINING_CHARS: usize = 8;
const MAX_TRAINING_CHARS: usize = 180;
const MAX_TRAINING_WORDS: usize = 22;

static MENTION_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"<@!?[0-9]+>").unwrap());
static SNOWFLAKE_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b[0-9]{17,20}\b").unwrap());

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingPair {
    pub input: String,
    pub output: String,
    pub input_author: String,
    pub output_author: String,
    pub message_id: String,
    pub quality_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoreStats {
    pub total_seen: u64,
    pub saved: u64,
    pub training_pairs_saved: u64,
    pub skipped_bot: u64,
    pub skipped_empty: u64,
    pub skipped_duplicate_id: u64,
    pub skipped_duplicate_content: u64,
    pub skipped_low_quality: u64,
    pub skipped_spam: u64,
    pub skipped_toxic: u64,
    pub skipped_too_short: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessOutcome {
    Saved,
    SkippedBot,
    SkippedEmpty,
    SkippedDuplicateId,
    SkippedDuplicateContent,
    SkippedLowQuality,
    SkippedSpam,
    SkippedToxic,
    SkippedTooShort,
}

pub struct MessageStore {
    data_dir: PathBuf,
    seen_ids: HashSet<String>,
    seen_content_hashes: HashSet<String>,
    seen_training_hashes: HashSet<String>,
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
        let seen_training_hashes =
            load_string_set(&data_dir.join("state/seen_training_hashes.json")).await?;
        let stats = load_stats(&data_dir.join("reports/latest.json")).await?;

        Ok(Self {
            data_dir,
            seen_ids,
            seen_content_hashes,
            seen_training_hashes,
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

        if self.seen_ids.contains(&input.id) {
            self.stats.skipped_duplicate_id += 1;
            return Ok(ProcessOutcome::SkippedDuplicateId);
        }

        let content = match validate_content(&input.content) {
            Ok(cleaned) => cleaned,
            Err(reason) => {
                self.seen_ids.insert(input.id);
                self.record_skip(reason);
                return Ok(reason);
            }
        };

        let content_hash = hash_content(&content);
        if self.seen_content_hashes.contains(&content_hash) {
            self.stats.skipped_duplicate_content += 1;
            self.seen_ids.insert(input.id);
            return Ok(ProcessOutcome::SkippedDuplicateContent);
        }

        let quality_score = score_content(&content);
        if quality_score < MIN_QUALITY {
            self.stats.skipped_low_quality += 1;
            self.seen_ids.insert(input.id);
            return Ok(ProcessOutcome::SkippedLowQuality);
        }

        let reply_to = input.reply_to.and_then(|reply| {
            let cleaned_parent = validate_content(&reply.content).ok()?;
            if cleaned_parent.is_empty() {
                return None;
            }
            Some(ReplyContext {
                message_id: reply.message_id,
                author_id: reply.author_id,
                author_name: reply.author_name,
                content: cleaned_parent,
            })
        });

        let stored = StoredMessage {
            id: input.id.clone(),
            author_id: input.author_id.clone(),
            author_name: input.author_name.clone(),
            content: content.clone(),
            channel_id: input.channel_id,
            guild_id: input.guild_id,
            timestamp: input.timestamp,
            reply_to: reply_to.clone(),
            quality_score,
        };

        self.append_message(&stored).await?;
        self.maybe_save_training_pair(&stored, &reply_to, quality_score)
            .await?;

        self.seen_ids.insert(input.id);
        self.seen_content_hashes.insert(content_hash);
        self.stats.saved += 1;
        self.persist_state().await?;

        Ok(ProcessOutcome::Saved)
    }

    pub fn stats(&self) -> &StoreStats {
        &self.stats
    }

    fn record_skip(&mut self, reason: ProcessOutcome) {
        match reason {
            ProcessOutcome::SkippedEmpty => self.stats.skipped_empty += 1,
            ProcessOutcome::SkippedSpam => self.stats.skipped_spam += 1,
            ProcessOutcome::SkippedToxic => self.stats.skipped_toxic += 1,
            ProcessOutcome::SkippedTooShort => self.stats.skipped_too_short += 1,
            ProcessOutcome::SkippedLowQuality => self.stats.skipped_low_quality += 1,
            _ => {}
        }
    }

    async fn maybe_save_training_pair(
        &mut self,
        stored: &StoredMessage,
        reply_to: &Option<ReplyContext>,
        output_quality: f32,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Some(parent) = reply_to else {
            return Ok(());
        };

        if parent.content.chars().count() < MIN_TRAINING_CHARS
            || stored.content.chars().count() < MIN_TRAINING_CHARS
            || parent.content.chars().count() > MAX_TRAINING_CHARS
            || stored.content.chars().count() > MAX_TRAINING_CHARS
            || parent.content.split_whitespace().count() > MAX_TRAINING_WORDS
            || stored.content.split_whitespace().count() > MAX_TRAINING_WORDS
        {
            return Ok(());
        }

        let input_quality = score_content(&parent.content);
        if input_quality < MIN_TRAINING_QUALITY || output_quality < MIN_TRAINING_QUALITY {
            return Ok(());
        }

        if is_laughter_spam(&parent.content) || is_laughter_spam(&stored.content) {
            return Ok(());
        }

        if is_toxic(&parent.content) || is_toxic(&stored.content) {
            return Ok(());
        }

        let pair_hash = hash_content(&format!("{}=>{}", parent.content, stored.content));
        if self.seen_training_hashes.contains(&pair_hash) {
            return Ok(());
        }

        let pair = TrainingPair {
            input: parent.content.clone(),
            output: stored.content.clone(),
            input_author: parent.author_name.clone(),
            output_author: stored.author_name.clone(),
            message_id: stored.id.clone(),
            quality_score: (input_quality + output_quality) / 2.0,
        };

        let path = self.data_dir.join("clean/training_pairs.jsonl");
        let line = serde_json::to_string(&pair)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?;
        file.write_all(format!("{line}\n").as_bytes()).await?;

        self.seen_training_hashes.insert(pair_hash);
        self.stats.training_pairs_saved += 1;
        Ok(())
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
        save_string_set(
            &self.data_dir.join("state/seen_training_hashes.json"),
            &self.seen_training_hashes,
        )
        .await?;

        let report = serde_json::json!({
            "stats": self.stats,
            "unique_messages": self.seen_ids.len(),
            "unique_content": self.seen_content_hashes.len(),
            "unique_training_pairs": self.seen_training_hashes.len(),
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
    normalize_text(text, &MENTION_REGEX, &SNOWFLAKE_REGEX)
}

fn normalize_text(text: &str, mention_regex: &Regex, snowflake_regex: &Regex) -> String {
    let without_mentions = mention_regex.replace_all(text, " ");
    let collapsed = collapse_repetitions(&without_mentions);
    let filtered: String = collapsed
        .chars()
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
                    | ':'
                    | ';'
                    | '-'
                    | '_'
            )
        })
        .collect();

    let without_ids = snowflake_regex.replace_all(&filtered, " ");
    without_ids
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn validate_content(text: &str) -> Result<String, ProcessOutcome> {
    let content = normalize_text(text, &MENTION_REGEX, &SNOWFLAKE_REGEX);

    if content.is_empty() {
        return Err(ProcessOutcome::SkippedEmpty);
    }

    if is_toxic(&content) {
        return Err(ProcessOutcome::SkippedToxic);
    }

    if is_laughter_spam(&content) || is_mostly_extended_chars(&content) {
        return Err(ProcessOutcome::SkippedSpam);
    }

    if has_excessive_repetition(&content) {
        return Err(ProcessOutcome::SkippedSpam);
    }

    if content.chars().count() < MIN_CHARS {
        return Err(ProcessOutcome::SkippedTooShort);
    }

    if is_mostly_noise(&content) {
        return Err(ProcessOutcome::SkippedLowQuality);
    }

    Ok(content)
}

fn collapse_repetitions(content: &str) -> String {
    let chars: Vec<char> = content.chars().collect();
    let mut output = String::new();
    let mut index = 0;

    while index < chars.len() {
        let current = chars[index];
        let mut run = 1;
        while index + run < chars.len() && chars[index + run] == current {
            run += 1;
        }

        for _ in 0..run.min(3) {
            output.push(current);
        }

        index += run;
    }

    output
}

fn score_content(content: &str) -> f32 {
    let char_count = content.chars().count();
    let word_count = content.split_whitespace().count();
    let mut score: f32 = 0.5;

    if char_count >= MIN_CHARS {
        score += 0.1;
    }
    if char_count >= 12 {
        score += 0.1;
    }
    if word_count >= 2 {
        score += 0.15;
    }
    if word_count >= 4 {
        score += 0.1;
    }
    if contains_arabic(content) {
        score += 0.1;
    }
    if content.contains('?') {
        score += 0.05;
    }

    if has_excessive_repetition(content) {
        score -= 0.4;
    }
    if is_laughter_spam(content) {
        score -= 0.5;
    }
    if is_toxic(content) {
        score -= 0.8;
    }
    if char_count < MIN_CHARS {
        score -= 0.4;
    }

    score.clamp(0.0, 1.0)
}

fn is_laughter_spam(content: &str) -> bool {
    let compact: String = content.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.is_empty() {
        return false;
    }

    let laugh_chars = compact
        .chars()
        .filter(|c| matches!(c, 'ه' | 'ح' | 'h' | 'H'))
        .count();

    laugh_chars as f32 / compact.chars().count() as f32 > 0.55
}

fn is_mostly_extended_chars(content: &str) -> bool {
    let compact: String = content.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.len() < 8 {
        return false;
    }

    let first = compact.chars().next().unwrap_or(' ');
    compact.chars().filter(|c| *c == first).count() as f32 / compact.chars().count() as f32 > 0.75
}

fn is_toxic(content: &str) -> bool {
    let normalized = normalize_for_toxic_check(content);

    const BLOCKED: &[&str] = &[
        "خرا",
        "الخرا",
        "لعنه",
        "لعنة",
        "لعن",
        "كس",
        "طيز",
        "زق",
        "على زق",
        "قذف",
        "قح",
        "قحب",
        "شرمو",
        "معرص",
        "ادعس",
        "انيك",
        "نيك",
        "ياكلب",
        "كلب",
        "وصخ",
        "وسخ",
        "سدحلق",
        "لحس",
        "متناك",
        "منيوك",
        "fuck",
        "shit",
        "bitch",
        "asshole",
        "dick",
    ];

    if BLOCKED.iter().any(|word| normalized.contains(word)) {
        return true;
    }

    // Insult patterns that are common in Saudi chat.
    let insult_patterns = [
        " ادعس ",
        " على زق",
        " ياكلب",
        " يا كلب",
        " وصخ",
        " وسخ",
        " سدح ",
        " قذف ",
    ];

    let padded = format!(" {normalized} ");
    insult_patterns.iter().any(|pattern| padded.contains(pattern))
}

fn normalize_for_toxic_check(content: &str) -> String {
    let mut normalized = content.to_lowercase();

    for (from, to) in [
        ('أ', "ا"),
        ('إ', "ا"),
        ('آ', "ا"),
        ('ة', "ه"),
        ('ى', "ي"),
        ('ؤ', "و"),
        ('ئ', "ي"),
    ] {
        normalized = normalized.replace(from, to);
    }

    collapse_repetitions(&normalized)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_mostly_noise(content: &str) -> bool {
    let letters: Vec<char> = content
        .chars()
        .filter(|c| c.is_alphanumeric() || ('\u{0600}'..='\u{06FF}').contains(c))
        .collect();

    if letters.is_empty() {
        return true;
    }

    let unique = letters.iter().collect::<HashSet<_>>().len();
    unique <= 2 && letters.len() > 6
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

    max_run >= 5
}

fn contains_arabic(content: &str) -> bool {
    content.chars().any(|c| ('\u{0600}'..='\u{06FF}').contains(&c))
}

fn hash_content(content: &str) -> String {
    let normalized = content.to_lowercase();
    let digest = Sha256::digest(normalized.as_bytes());
    format!("{:x}", digest)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_snowflakes_and_spam() {
        let cleaned = clean_text("1205281897635389490 تعالي");
        assert_eq!(cleaned, "تعالي");

        assert!(validate_content("هههههههههههههههههههه").is_err());
        assert!(validate_content("896240360077025281 ادعس لونك الخرا").is_err());
        assert!(validate_content("Dody على زق").is_err());
        assert!(validate_content("ياكلب ههه").is_err());
        assert!(validate_content("يب يب وصخ").is_err());
        assert!(validate_content("نعم").is_err());
        assert!(validate_content("وش ودك").is_ok());
        assert!(validate_content("السلام عليكم").is_ok());
    }
}
