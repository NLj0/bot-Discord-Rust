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
use std::time::{Duration, Instant};
use reqwest::Client;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use sqlx::mysql::MySqlPool;
use std::sync::Arc;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use serenity::all::CreateMessage;
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
        r"(?i)(https?://)?(?:www\.)?([a-zA-Z0-9\-]+\.)+[a-zA-Z]{2,}(?:/[^\s]*)?"
    ).unwrap();
}

pub struct URLExtractor;

impl URLExtractor {
    /// استخراج جميع الروابط من الرسالة وتطبيع البروتوكول
    pub fn extract_urls(text: &str) -> Vec<String> {
        URL_REGEX
            .find_iter(text)
            .map(|m| {
                let raw = m.as_str();
                if raw.to_lowercase().starts_with("http://") || raw.to_lowercase().starts_with("https://") {
                    raw.to_string()
                } else {
                    format!("https://{}", raw)
                }
            })
            .collect()
    }

    /// هل الرسالة تحتوي على روابط؟
    #[allow(dead_code)]
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
        // استخراج الجزء بعد http(s)://
        let host = url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap_or("")
            .split(':')
            .next()
            .unwrap_or("");

        // كشف IPv4: أربعة أقسام من الأرقام مفصولة بنقاط
        let parts: Vec<&str> = host.split('.').collect();
        if parts.len() == 4 {
            return parts.iter().all(|p| p.parse::<u8>().is_ok());
        }

        // كشف IPv6: يبدأ بـ [
        if host.starts_with('[') {
            return true;
        }

        false
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
                println!("[TYPOSQUAT] '{}' resembles '{}' (distance={} char)", domain_name, target, dist);
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
    /// Is this URL known to be dangerous?
    pub fn is_blacklisted(url: &str) -> bool {
        let lower = url.to_lowercase();
        let domain = RiskScorer::extract_domain(&lower);
        let tld = domain.split('.').last().unwrap_or("");

        // 1. Explicit malicious keywords in URL (path or domain)
        const MALICIOUS_KEYWORDS: &[&str] = &[
            "phishing", "malware", "scam", "hack", "crack",
            "free-nitro", "freenitro", "discord-gift", "discordgift",
            "free-robux", "freerobux", "freevbucks", "vbucks-free",
            "login-verify", "verify-login", "account-suspended",
            "confirm-identity", "suspended-account", "wallet-connect",
            "claim-prize", "you-won", "verify-human",
        ];
        if MALICIOUS_KEYWORDS.iter().any(|k| lower.contains(k)) {
            return true;
        }

        // 2. Known IP-logger / grabify services
        const IP_LOGGERS: &[&str] = &[
            "grabify.link", "iplogger.org", "iplogger.com", "iplogger.ru",
            "2no.co", "yip.su", "bmwforum.co", "leancoding.co",
            "youripleak.com", "blasze.tk", "ps3cfw.com", "api.grabify",
            "ezstat.ru", "lovelocator.net", "geolocation.live", "ssregistry.com",
        ];
        if IP_LOGGERS.iter().any(|b| domain.contains(b)) {
            return true;
        }

        // 3. Discord / gaming scam domain patterns
        const SCAM_PATTERNS: &[&str] = &[
            "discord-nitro", "discordnitro", "discord-free",
            "steam-community", "steamcommunity-", "steam-gift", "steamgift",
            "roblox-free", "robux-free", "free-robux",
            "csgo-", "cs-go-", "free-skin", "freeskin",
            "epicgames-free", "free-epic",
        ];
        if SCAM_PATTERNS.iter().any(|p| domain.contains(p)) {
            return true;
        }

        // 4. Suspicious free TLDs + additional signal (long or hyphenated domain)
        const SUSPICIOUS_TLDS: &[&str] = &["tk", "ml", "ga", "cf", "gq", "pw", "click", "link"];
        let suspicious_tld = SUSPICIOUS_TLDS.contains(&tld);
        let hyphen_count = domain.chars().filter(|&c| c == '-').count();
        let domain_len = domain.len();
        if suspicious_tld && (hyphen_count >= 2 || domain_len > 25) {
            return true;
        }

        // 5. Excessive subdomain depth (4+ dots = likely DGA or phishing host)
        if domain.chars().filter(|&c| c == '.').count() >= 4 {
            return true;
        }

        // 6. Long subdomain on free hosting platforms (common phishing vector)
        const FREE_HOSTS: &[&str] = &[
            "000webhostapp.com", "weebly.com", "wixsite.com",
            "netlify.app", "vercel.app", "pages.dev",
            "glitch.me", "repl.co", "web.app",
        ];
        if FREE_HOSTS.iter().any(|h| domain.ends_with(h)) && domain_len > 30 {
            return true;
        }

        false
    }

    /// Is this URL known to be safe?
    pub fn is_whitelisted(url: &str) -> bool {
        let domain = RiskScorer::extract_domain(url);
        const WHITELIST: &[&str] = &[
            "github.com",
            "rust-lang.org",
            "google.com",
            "stackoverflow.com",
            "discord.com",
            "youtube.com",
            "twitch.tv",
            "reddit.com",
            "wikipedia.org",
            "mozilla.org",
            "microsoft.com",
            "npmjs.com",
            "crates.io",
            "docs.rs",
        ];
        // Match exact domain or any subdomain (e.g. docs.github.com)
        WHITELIST.iter().any(|w| domain == *w || domain.ends_with(&format!(".{}", w)))
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
    #[allow(dead_code)]
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
        println!("[VIRUSTOTAL] Submitted for analysis: {}", url);
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
                println!("[WHITELIST:DB] ✅ whitelisted: {}", domain);
                let result = LinkCheckResult {
                    url: url.to_string(),
                    level: SecurityLevel::Whitelist,
                    risk_score: 0.0,
                    is_safe: true,
                    reason: format!("Whitelisted: {}", domain),
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
                reason: "Known blacklisted URL".to_string(),
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
                reason: "In trusted whitelist".to_string(),
            };
            LinkCache::set(url, result.clone(), self.cache_ttl);
            return result;
        }

        // حساب درجة الخطر الأولية
        let mut risk_score = RiskScorer::calculate(url);

        // Level 1: Google Safe Browsing
        if let Some(ref google) = self.google {
            let gsb_start = Instant::now();
            match google.check(url).await {
                Ok(is_safe) => {
                    println!("[GOOGLE] ⏱️ {}ms | {} | {}",
                        gsb_start.elapsed().as_millis(),
                        url,
                        if is_safe { "✅ Safe" } else { "🚨 THREAT DETECTED" }
                    );
                    if !is_safe {
                        let result = LinkCheckResult {
                            url: url.to_string(),
                            level: SecurityLevel::Blacklist,
                            risk_score: 0.95,
                            is_safe: false,
                            reason: "Google Safe Browsing: threat detected".to_string(),
                        };
                        self.save(&result).await;
                        LinkCache::set(url, result.clone(), self.cache_ttl);
                        return result;
                    }
                }
                Err(e) => {
                    println!("[GOOGLE] ⏱️ {}ms | {} | ❌ Error: {}",
                        gsb_start.elapsed().as_millis(),
                        url,
                        e
                    );
                }
            }
        } else {
            println!("[GOOGLE] ⚠️ Skipped (no API key) | {}", url);
        }

        // Level 2: VirusTotal — إذا وصلنا هنا فـ Google لم يحظر الرابط
        if let Some(ref vt) = self.virustotal {
            let vt_start = Instant::now();
            match vt.check(url).await {
                Ok(is_safe) => {
                    println!("[VIRUSTOTAL] ⏱️ {}ms | {} | {}",
                        vt_start.elapsed().as_millis(),
                        url,
                        if is_safe { "✅ Safe" } else { "🚨 THREAT DETECTED" }
                    );
                    if !is_safe {
                        let result = LinkCheckResult {
                            url: url.to_string(),
                            level: SecurityLevel::Blacklist,
                            risk_score: 0.9,
                            is_safe: false,
                            reason: "VirusTotal: threat detected".to_string(),
                        };
                        self.save(&result).await;
                        LinkCache::set(url, result.clone(), self.cache_ttl);
                        return result;
                    }
                    risk_score = (risk_score * 0.5).min(0.3);
                }
                Err(e) => {
                    println!("[VIRUSTOTAL] ❌ Error: {}", e);
                    risk_score = (risk_score + 0.3).min(1.0);
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
                "No threats detected".to_string()
            } else {
                format!("Risk score: {:.0}%", risk_score * 100.0)
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
                println!("[DB] Failed to save URL: {}", e);
            } else {
                println!("[DB] 💾 Saved: {} | {}", result.url, classification);
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

// نظام Rate Limiting لحماية API من البريد المزعج
static RATE_LIMIT: Lazy<DashMap<u64, Instant>> = Lazy::new(DashMap::new);
const RATE_LIMIT_COOLDOWN: Duration = Duration::from_secs(5);

// dedup لمنع معالجة نفس الرسالة مرتين
static SEEN_MESSAGES: Lazy<DashMap<u64, Instant>> = Lazy::new(DashMap::new);

fn is_rate_limited(user_id: u64) -> bool {
    let now = Instant::now();
    if let Some(last) = RATE_LIMIT.get(&user_id) {
        if now.duration_since(*last) < RATE_LIMIT_COOLDOWN {
            return true;
        }
    }
    RATE_LIMIT.insert(user_id, now);
    false
}

/// تنظيف دوري: حذف المدخلات المنتهية من الذاكرة تجنباً لتراكم غير محدود
fn cleanup_rate_limit() {
    let now = Instant::now();
    RATE_LIMIT.retain(|_, last| now.duration_since(*last) < Duration::from_secs(60));
    SEEN_MESSAGES.retain(|_, last| now.duration_since(*last) < Duration::from_secs(10));
}

pub async fn handle_message(ctx: &Context, msg: &Message, engine: &LinkSecurityEngine) {
    if msg.author.bot { return; }

    // تنظيف دوري للذاكرة
    cleanup_rate_limit();

    // Rate Limiting: تجاهل المستخدم إذا أرسل روابط بسرعة كبيرة
    let user_id = msg.author.id.get();
    if is_rate_limited(user_id) {
        println!("[RATE LIMIT] Skipped scan for user_id={}", user_id);
        return;
    }

    // Dedup: تجاهل نفس الرسالة إذا عولجت مسبقاً (Discord أحياناً يرسل event مرتين)
    let msg_id = msg.id.get();
    if SEEN_MESSAGES.insert(msg_id, Instant::now()).is_some() {
        return;
    }

    match engine.scan_message(msg).await {
        Some(Action::Delete) => {
            let _ = msg.delete(ctx).await;
            // إرسال تنبيه خاص
            if let Ok(dm) = msg.author.create_dm_channel(ctx).await {
                let _ = dm.id.send_message(&ctx.http, CreateMessage::new().content("🛡️ تم حذف رسالتك لأنها تحتوي على روابط خطيرة")).await;
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
