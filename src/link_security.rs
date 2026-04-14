/// 🛡️ Link Security System
/// فحص تلقائي للروابط في جميع الرسائل بدون أوامر
/// Auto checks all URLs in messages without commands

use serenity::client::Context;
use serenity::model::prelude::Message;
use regex::Regex;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use lazy_static::lazy_static;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::time::Duration;
use std::time::Instant;
use reqwest::Client;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use sqlx::mysql::MySqlPool;
use std::sync::Arc;
use crate::database::{db_get_url, db_save_url, is_whitelisted};

// ═══════════════════════════════════════════════════════════════════════
// 📊 DATA STRUCTURES
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecurityLevel {
    /// Level 0: Blacklist - حظر فوري
    Blacklist,
    /// Level 1: Unknown - فحص Google
    Unknown,
    /// Level 2: Whitelist - معروف وآمن
    Whitelist,
    /// Level 3: Suspicious - فحص عميق VirusTotal
    Suspicious,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkCheckResult {
    pub url: String,
    pub level: SecurityLevel,
    pub risk_score: f32,
    pub is_safe: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum Action {
    Allow,
    Warn,
    Delete,
}

// ═══════════════════════════════════════════════════════════════════════
// 🔍 URL EXTRACTOR
// ═══════════════════════════════════════════════════════════════════════

lazy_static! {
    static ref URL_REGEX: Regex = Regex::new(
        r"https?://[^\s]+"
    ).unwrap();
}

pub struct URLExtractor;

impl URLExtractor {
    /// استخراج جميع الروابط من الرسالة
    pub fn extract_urls(text: &str) -> Vec<String> {
        URL_REGEX
            .find_iter(text)
            .map(|m| m.as_str().to_string())
            .collect()
    }

    /// هل الرسالة تحتوي على روابط؟
    pub fn has_urls(text: &str) -> bool {
        !Self::extract_urls(text).is_empty()
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 💾 CACHE SYSTEM
// ═══════════════════════════════════════════════════════════════════════

lazy_static! {
    static ref CACHE: RwLock<HashMap<String, CachedResult>> = RwLock::new(HashMap::new());
}

struct CachedResult {
    result: LinkCheckResult,
    expires_at: i64,
}

pub struct LinkCache;

impl LinkCache {
    pub fn get(url: &str) -> Option<LinkCheckResult> {
        let mut cache = CACHE.write();
        let now = Utc::now().timestamp();

        if let Some(cached) = cache.get(url) {
            if cached.expires_at > now {
                return Some(cached.result.clone());
            } else {
                cache.remove(url);
            }
        }
        None
    }

    pub fn set(url: &str, result: LinkCheckResult, ttl_hours: i32) {
        let expires_at = Utc::now().timestamp() + (ttl_hours as i64 * 3600);
        let cached = CachedResult { result, expires_at };
        CACHE.write().insert(url.to_lowercase(), cached);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 📊 RISK SCORING
// ═══════════════════════════════════════════════════════════════════════

pub struct RiskScorer;

impl RiskScorer {
    /// حساب درجة الخطر (0.0 - 1.0)
    pub fn calculate(url: &str) -> f32 {
        let mut score: f32 = 0.0;

        if Self::is_shortlink(url)           { score += 0.2; }
        if Self::is_ip_based(url)            { score += 0.3; }
        if Self::has_suspicious_protocol(url){ score += 0.25; }
        if Self::is_typosquatting(url)       { score += 0.85; }

        score.min(1.0)
    }

    fn is_shortlink(url: &str) -> bool {
        let shorteners = vec![
            "bit.ly", "tinyurl.com", "ow.ly", "goo.gl", "is.gd", "buff.ly"
        ];
        shorteners.iter().any(|s| url.contains(s))
    }

    fn is_ip_based(url: &str) -> bool {
        url.contains("192.168.") || url.contains("10.") || url.contains("172.")
    }

    fn has_suspicious_protocol(url: &str) -> bool {
        url.starts_with("data:") || url.starts_with("javascript:")
    }

    pub fn extract_domain(url: &str) -> String {
        url.trim_start_matches("https://")
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap_or("")
            .split('?')
            .next()
            .unwrap_or("")
            .to_lowercase()
    }

    fn levenshtein(a: &str, b: &str) -> usize {
        let a: Vec<char> = a.chars().collect();
        let b: Vec<char> = b.chars().collect();
        let (m, n) = (a.len(), b.len());
        if m == 0 { return n; }
        if n == 0 { return m; }
        let mut dp = vec![vec![0usize; n + 1]; m + 1];
        for i in 0..=m { dp[i][0] = i; }
        for j in 0..=n { dp[0][j] = j; }
        for i in 1..=m {
            for j in 1..=n {
                dp[i][j] = if a[i-1] == b[j-1] {
                    dp[i-1][j-1]
                } else {
                    1 + dp[i-1][j].min(dp[i][j-1]).min(dp[i-1][j-1])
                };
            }
        }
        dp[m][n]
    }

    fn is_typosquatting(url: &str) -> bool {
        let domain = Self::extract_domain(url);
        let domain_name = domain.split('.').next().unwrap_or("");
        if domain_name.is_empty() { return false; }

        let known = [
            "discord", "youtube", "google", "github",
            "twitter", "facebook", "instagram", "twitch",
            "reddit", "microsoft", "apple", "amazon",
            "netflix", "steam", "roblox", "paypal", "binance",
        ];

        for target in &known {
            if domain_name == *target { continue; }
            let dist = Self::levenshtein(domain_name, target);
            if dist >= 1 && dist <= 2 {
                println!("[TYPOSQUAT] '{}' يشبه '{}' (فرق={} حرف)", domain_name, target, dist);
                return true;
            }
        }
        false
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 🔴 BLACKLIST / WHITELIST
// ═══════════════════════════════════════════════════════════════════════

pub struct SecurityList;

impl SecurityList {
    /// معروف أنه خطر؟
    pub fn is_blacklisted(url: &str) -> bool {
        let blacklist = vec![
            "phishing",
            "malware",
            "scam",
        ];
        blacklist.iter().any(|b| url.to_lowercase().contains(b))
    }

    /// معروف أنه آمن؟
    pub fn is_whitelisted(url: &str) -> bool {
        let whitelist = vec![
            "github.com",
            "rust-lang.org",
            "google.com",
            "stackoverflow.com",
            "discord.com",
        ];
        whitelist.iter().any(|w| url.to_lowercase().contains(w))
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 🌐 GOOGLE SAFE BROWSING
// ═══════════════════════════════════════════════════════════════════════

pub struct GoogleSafeBrowsing {
    api_key: String,
    client: Client,
}

#[derive(Debug, Serialize)]
struct GoogleRequest {
    client: GoogleClientInfo,
    threat_info: GoogleThreatInfo,
}

#[derive(Debug, Serialize)]
struct GoogleClientInfo {
    client_id: String,
    client_version: String,
}

#[derive(Debug, Serialize)]
struct GoogleThreatInfo {
    threat_types: Vec<String>,
    platform_types: Vec<String>,
    threat_entry_types: Vec<String>,
    threat_entries: Vec<GoogleThreatEntry>,
}

#[derive(Debug, Serialize)]
struct GoogleThreatEntry {
    url: String,
}

#[derive(Debug, Deserialize)]
struct GoogleResponse {
    matches: Option<Vec<GoogleMatch>>,
}

#[derive(Debug, Deserialize)]
struct GoogleMatch {
    threat_type: String,
}

impl GoogleSafeBrowsing {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: Client::new(),
        }
    }

    pub async fn check(&self, url: &str) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let request = GoogleRequest {
            client: GoogleClientInfo {
                client_id: "discord-bot".to_string(),
                client_version: "1.0".to_string(),
            },
            threat_info: GoogleThreatInfo {
                threat_types: vec![
                    "MALWARE".to_string(),
                    "SOCIAL_ENGINEERING".to_string(),
                ],
                platform_types: vec!["ANY_PLATFORM".to_string()],
                threat_entry_types: vec!["URL".to_string()],
                threat_entries: vec![GoogleThreatEntry {
                    url: url.to_string(),
                }],
            },
        };

        let response = self
            .client
            .post(&format!(
                "https://safebrowsing.googleapis.com/v4/threatMatches:find?key={}",
                self.api_key
            ))
            .json(&request)
            .send()
            .await?;

        let body: GoogleResponse = response.json().await?;
        Ok(body.matches.is_none())
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 🦠 VIRUSTOTAL
// ═══════════════════════════════════════════════════════════════════════

pub struct VirusTotal {
    api_key: String,
    client: Client,
}

#[derive(Debug, Deserialize)]
struct VTResponse {
    data: Option<VTData>,
}

#[derive(Debug, Deserialize)]
struct VTData {
    attributes: VTAttributes,
}

#[derive(Debug, Deserialize)]
struct VTAttributes {
    last_analysis_stats: VTStats,
}

#[derive(Debug, Deserialize)]
struct VTStats {
    malicious: i32,
    suspicious: i32,
}

impl VirusTotal {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
        }
    }

    pub async fn check(&self, url: &str) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        // VT API v3 يحتاج base64url encoding وليس percent-encoding
        let url_id = URL_SAFE_NO_PAD.encode(url.trim_end_matches('/').as_bytes());

        let response = self
            .client
            .get(&format!(
                "https://www.virustotal.com/api/v3/urls/{}",
                url_id
            ))
            .header("x-apikey", &self.api_key)
            .send()
            .await?;

        // 404 = الرابط مو في قاعدة VT → نرسله للتحليل
        if response.status() == 404 {
            let _ = self.submit(url).await;
            return Err("URL not in VirusTotal database".into());
        }

        let body: VTResponse = response.json().await?;

        if let Some(data) = body.data {
            let stats = data.attributes.last_analysis_stats;
            Ok(stats.malicious == 0 && stats.suspicious == 0)
        } else {
            Err("No analysis data".into())
        }
    }

    async fn submit(&self, url: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let params = [("url", url)];
        self.client
            .post("https://www.virustotal.com/api/v3/urls")
            .header("x-apikey", &self.api_key)
            .form(&params)
            .send()
            .await?;
        println!("[VIRUSTOTAL] تم إرسال الرابط للتحليل: {}", url);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 🎯 MAIN SECURITY ENGINE
// ═══════════════════════════════════════════════════════════════════════

pub struct LinkSecurityEngine {
    google: Option<GoogleSafeBrowsing>,
    virustotal: Option<VirusTotal>,
    pool: Option<Arc<MySqlPool>>,
    cache_ttl: i32,
    auto_delete: bool,
}

impl LinkSecurityEngine {
    pub fn new(
        google_key: Option<String>,
        virustotal_key: Option<String>,
        pool: Option<Arc<MySqlPool>>,
        cache_ttl: i32,
        auto_delete: bool,
    ) -> Self {
        Self {
            google: google_key.map(GoogleSafeBrowsing::new),
            virustotal: virustotal_key.map(VirusTotal::new),
            pool,
            cache_ttl,
            auto_delete,
        }
    }

    /// فحص رابط واحد
    pub async fn check_url(&self, url: &str) -> LinkCheckResult {
        // 0️⃣ تحقق من الـ DB whitelist أولاً (يتجاوز الكاش تماماً)
        if let Some(ref pool) = self.pool {
            let domain = RiskScorer::extract_domain(url);
            if is_whitelisted(pool, &domain).await {
                println!("[WHITELIST:DB] ✅ {}", domain);
                let result = LinkCheckResult {
                    url: url.to_string(),
                    level: SecurityLevel::Whitelist,
                    risk_score: 0.0,
                    is_safe: true,
                    reason: format!("في القائمة البيضاء: {}", domain),
                };
                LinkCache::set(url, result.clone(), self.cache_ttl);
                return result;
            }
        }

        // 1️⃣ تحقق من الـ in-memory cache أولاً (أسرع)
        if let Some(cached) = LinkCache::get(url) {
            println!("[CACHE:RAM] ✅ hit: {}", url);
            return cached;
        }

        // 2️⃣ تحقق من الداتا بيس
        if let Some(ref pool) = self.pool {
            if let Some(cached) = db_get_url(pool, url).await {
                println!("[CACHE:DB] ✅ hit: {} | safe={} | {}", url, cached.is_safe, cached.reason);
                let level = match cached.classification.as_str() {
                    "Blacklist"  => SecurityLevel::Blacklist,
                    "Whitelist"  => SecurityLevel::Whitelist,
                    "Suspicious" => SecurityLevel::Suspicious,
                    _            => SecurityLevel::Unknown,
                };
                let result = LinkCheckResult {
                    url: url.to_string(),
                    level,
                    risk_score: cached.risk_score,
                    is_safe: cached.is_safe,
                    reason: cached.reason,
                };
                LinkCache::set(url, result.clone(), self.cache_ttl);
                return result;
            }
        }

        // Level 0: Blacklist
        if SecurityList::is_blacklisted(url) {
            let result = LinkCheckResult {
                url: url.to_string(),
                level: SecurityLevel::Blacklist,
                risk_score: 1.0,
                is_safe: false,
                reason: "معروف في قائمة الحظر".to_string(),
            };
            LinkCache::set(url, result.clone(), self.cache_ttl);
            return result;
        }

        // Level 2: Whitelist ثابتة
        if SecurityList::is_whitelisted(url) {
            let result = LinkCheckResult {
                url: url.to_string(),
                level: SecurityLevel::Whitelist,
                risk_score: 0.1,
                is_safe: true,
                reason: "في قائمة الثقة".to_string(),
            };
            LinkCache::set(url, result.clone(), self.cache_ttl);
            return result;
        }

        // حساب درجة الخطر الأولية
        let mut risk_score = RiskScorer::calculate(url);

        // Level 1: Google Safe Browsing
        let mut google_flagged = false;
        if let Some(ref google) = self.google {
            match google.check(url).await {
                Ok(is_safe) => {
                    if !is_safe {
                        google_flagged = true;
                        let result = LinkCheckResult {
                            url: url.to_string(),
                            level: SecurityLevel::Blacklist,
                            risk_score: 0.95,
                            is_safe: false,
                            reason: "Google Safe Browsing: الرابط خطر".to_string(),
                        };
                        self.save(&result).await;
                        LinkCache::set(url, result.clone(), self.cache_ttl);
                        return result;
                    }
                }
                Err(e) => {
                    println!("[GOOGLE] خطأ: {}", e);
                }
            }
        }

        // Level 2: VirusTotal — يفحص كل رابط غريب (مو في whitelist)
        if !google_flagged {
            if let Some(ref vt) = self.virustotal {
                let vt_start = Instant::now();
                match vt.check(url).await {
                    Ok(is_safe) => {
                        println!("[VIRUSTOTAL] ⏱️ {}ms | {} | {}",
                            vt_start.elapsed().as_millis(),
                            url,
                            if is_safe { "✅ آمن" } else { "🚨 خطر" }
                        );
                        if !is_safe {
                            let result = LinkCheckResult {
                                url: url.to_string(),
                                level: SecurityLevel::Blacklist,
                                risk_score: 0.9,
                                is_safe: false,
                                reason: "VirusTotal: الرابط خطر".to_string(),
                            };
                            self.save(&result).await;
                            LinkCache::set(url, result.clone(), self.cache_ttl);
                            return result;
                        }
                        risk_score = (risk_score * 0.5).min(0.3);
                    }
                    Err(e) => {
                        println!("[VIRUSTOTAL] {}", e);
                        risk_score = (risk_score + 0.3).min(1.0);
                    }
                }
            }
        }

        // القرار النهائي
        let (level, is_safe) = if risk_score > 0.7 {
            (SecurityLevel::Suspicious, false)
        } else if risk_score > 0.4 {
            (SecurityLevel::Suspicious, false)
        } else {
            (SecurityLevel::Unknown, true)
        };

        let result = LinkCheckResult {
            url: url.to_string(),
            level,
            risk_score,
            is_safe,
            reason: if is_safe {
                "لم يتم اكتشاف تهديدات".to_string()
            } else {
                format!("درجة الخطر: {:.0}%", risk_score * 100.0)
            },
        };

        LinkCache::set(url, result.clone(), self.cache_ttl);
        self.save(&result).await;
        result
    }

    /// حفظ النتيجة في الداتا بيس
    async fn save(&self, result: &LinkCheckResult) {
        if let Some(ref pool) = self.pool {
            let classification = match result.level {
                SecurityLevel::Blacklist  => "Blacklist",
                SecurityLevel::Whitelist  => "Whitelist",
                SecurityLevel::Suspicious => "Suspicious",
                SecurityLevel::Unknown    => "Unknown",
            };
            // آمن = 7 أيام، خطر = 30 يوم
            let ttl = if result.is_safe { 7 * 24 } else { 30 * 24 };
            if let Err(e) = db_save_url(
                pool,
                &result.url,
                result.is_safe,
                result.risk_score,
                classification,
                &result.reason,
                ttl,
            ).await {
                println!("[DB] خطأ في حفظ الرابط: {}", e);
            } else {
                println!("[DB] 💾 حُفظ: {} | {}", result.url, classification);
            }
        }
    }

    /// فحص الرسالة
    pub async fn scan_message(&self, msg: &Message) -> Option<Action> {
        // لا تفحص رسائل البوتات
        if msg.author.bot {
            return None;
        }

        // استخراج الروابط
        let urls = URLExtractor::extract_urls(&msg.content);
        println!("[SCAN] content={:?} urls_found={:?}", msg.content, urls);
        if urls.is_empty() {
            return None;
        }

        // فحص كل رابط
        for url in urls {
            let result = self.check_url(&url).await;
            println!("[CHECK] url={} safe={} score={:.2} reason={}", url, result.is_safe, result.risk_score, result.reason);

            // إذا الرابط خطر
            if !result.is_safe {
                return Some(if self.auto_delete {
                    Action::Delete
                } else {
                    Action::Warn
                });
            }

            // إذا الرابط مشبوه
            if result.level == SecurityLevel::Suspicious {
                return Some(Action::Warn);
            }
        }

        None
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 🔌 DISCORD INTEGRATION
// ═══════════════════════════════════════════════════════════════════════

pub async fn handle_message(ctx: &Context, msg: &Message, engine: &LinkSecurityEngine) {
    if msg.author.bot { return; }
    
    println!("[MSG] from={} content={:?}", msg.author.name, msg.content);

    match engine.scan_message(msg).await {
        Some(Action::Delete) => {
            let _ = msg.delete(ctx).await;
            // إرسال تنبيه خاص
            if let Ok(dm) = msg.author.create_dm_channel(ctx).await {
                let _ = dm.send_message(ctx, |m| {
                    m.content("🛡️ تم حذف رسالتك لأنها تحتوي على روابط خطيرة")
                }).await;
            }
        }
        Some(Action::Warn) => {
            let _ = msg.reply(ctx, "⚠️ **تحذير:** هذه الرسالة تحتوي على روابط مشبوهة، كن حذراً!").await;
        }
        _ => {}
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 📝 HELPER FUNCTIONS
// ═══════════════════════════════════════════════════════════════════════

pub fn create_engine(
    google_key: Option<String>,
    virustotal_key: Option<String>,
    pool: Arc<MySqlPool>,
) -> LinkSecurityEngine {
    LinkSecurityEngine::new(google_key, virustotal_key, Some(pool), 24, true)
}
