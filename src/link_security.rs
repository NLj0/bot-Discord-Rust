/// 🛡️ Link Security System — Deep HTTP Inspection Engine
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
use url::Url;
use sqlx::mysql::MySqlPool;
use std::sync::Arc;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use serenity::all::CreateMessage;
use crate::database::{db_get_url, db_save_url, is_whitelisted};

// ═══════════════════════════════════════════════════════════════════════
// 📊 DATA STRUCTURES
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    Safe,
    Malicious { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkCheckResult {
    pub url: String,
    pub verdict: Verdict,
    pub user_id: Option<u64>,
    pub timestamp: i64,
}

impl LinkCheckResult {
    pub fn is_safe(&self) -> bool {
        matches!(self.verdict, Verdict::Safe)
    }
    pub fn reason(&self) -> &str {
        match &self.verdict {
            Verdict::Safe => "No threats detected",
            Verdict::Malicious { reason } => reason.as_str(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum Action {
    Allow,
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
// 🔧 DOMAIN / IP HELPERS
// ═══════════════════════════════════════════════════════════════════════

pub struct RiskScorer;

impl RiskScorer {
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

    pub fn is_ip_based(url: &str) -> bool {
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
// 🔬 HTTP INSPECTOR — Deep HTTP Inspection Engine
// ═══════════════════════════════════════════════════════════════════════

const MAX_REDIRECTS: usize = 5;

pub struct HttpInspector {
    client: Client,
}

impl HttpInspector {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(4))
            .redirect(reqwest::redirect::Policy::none())
            .danger_accept_invalid_certs(true)
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36")
            .build()
            .unwrap_or_default();
        Self { client }
    }

    pub async fn inspect(&self, url: &str) -> Verdict {
        self.inspect_with_hops(url, 0).await
    }

    async fn inspect_with_hops(&self, url: &str, hops: usize) -> Verdict {
        // Step 1 — Protocol pre-check
        let lower = url.to_lowercase();
        if lower.starts_with("data:") || lower.starts_with("javascript:") {
            return Verdict::Malicious { reason: "Suspicious protocol".to_string() };
        }
        if RiskScorer::is_ip_based(url) {
            return Verdict::Malicious { reason: "IP-based URL".to_string() };
        }

        // Redirect depth guard
        if hops > MAX_REDIRECTS {
            return Verdict::Malicious { reason: "Too many redirects".to_string() };
        }

        // Step 2 — HTTP request (redirects handled manually)
        let response = match self.client.get(url).send().await {
            Ok(r) => r,
            Err(e) => {
                return Verdict::Malicious { reason: format!("Request failed: {}", e) };
            }
        };

        let status = response.status();

        // Step 3 — Manual redirect handling
        if status.is_redirection() {
            if let Some(location) = response.headers().get(reqwest::header::LOCATION) {
                if let Ok(location_str) = location.to_str() {
                    // Resolve the Location value against the current URL
                    let next_url = if let Ok(base) = Url::parse(url) {
                        base.join(location_str)
                            .map(|u| u.to_string())
                            .unwrap_or_else(|_| location_str.to_string())
                    } else {
                        location_str.to_string()
                    };

                    println!("[HTTP] ↪️ Redirect ({}) {} → {}", status.as_u16(), url, next_url);

                    // Blacklist-check the redirect target before following
                    if SecurityList::is_blacklisted(&next_url) {
                        return Verdict::Malicious { reason: format!("Redirects to blacklisted URL: {}", next_url) };
                    }

                    return Box::pin(self.inspect_with_hops(&next_url, hops + 1)).await;
                }
            }
            // Redirect with no Location header is suspicious
            return Verdict::Malicious { reason: format!("Redirect with no Location header ({})", status.as_u16()) };
        }

        // Step 4 — Inspect response body for phishing signals
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();

        // Only inspect HTML bodies
        if content_type.contains("html") {
            let body = match response.text().await {
                Ok(b) => b.to_lowercase(),
                Err(_) => return Verdict::Safe,
            };

            const PHISHING_SIGNALS: &[&str] = &[
                "enter your password",
                "verify your account",
                "confirm your identity",
                "your account has been suspended",
                "claim your prize",
                "you have been selected",
                "free nitro",
                "free robux",
                "wallet connect",
                "<input type=\"password\"",
                "input type=password",
            ];

            for signal in PHISHING_SIGNALS {
                if body.contains(signal) {
                    return Verdict::Malicious { reason: format!("Phishing page detected: '{}'", signal) };
                }
            }
        }

        Verdict::Safe
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 🎯 MAIN SECURITY ENGINE
// ═══════════════════════════════════════════════════════════════════════

pub struct LinkSecurityEngine {
    http_inspector: HttpInspector,
    pool: Option<Arc<MySqlPool>>,
    cache_ttl: i32,
}

impl LinkSecurityEngine {
    pub fn new(
        pool: Option<Arc<MySqlPool>>,
        cache_ttl: i32,
    ) -> Self {
        Self {
            http_inspector: HttpInspector::new(),
            pool,
            cache_ttl,
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
                    verdict: Verdict::Safe,
                    user_id: None,
                    timestamp: Utc::now().timestamp(),
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
                let verdict = if cached.is_safe {
                    Verdict::Safe
                } else {
                    Verdict::Malicious { reason: cached.reason.clone() }
                };
                let result = LinkCheckResult {
                    url: url.to_string(),
                    verdict,
                    user_id: None,
                    timestamp: Utc::now().timestamp(),
                };
                LinkCache::set(url, result.clone(), self.cache_ttl);
                return result;
            }
        }

        // Static blacklist check
        if SecurityList::is_blacklisted(url) {
            let result = LinkCheckResult {
                url: url.to_string(),
                verdict: Verdict::Malicious { reason: "Known blacklisted URL".to_string() },
                user_id: None,
                timestamp: Utc::now().timestamp(),
            };
            self.save(&result).await;
            LinkCache::set(url, result.clone(), self.cache_ttl);
            return result;
        }

        // Static whitelist check
        if SecurityList::is_whitelisted(url) {
            let result = LinkCheckResult {
                url: url.to_string(),
                verdict: Verdict::Safe,
                user_id: None,
                timestamp: Utc::now().timestamp(),
            };
            LinkCache::set(url, result.clone(), self.cache_ttl);
            return result;
        }

        // Deep HTTP inspection
        let inspect_start = Instant::now();
        let verdict = self.http_inspector.inspect(url).await;
        println!("[HTTP] ⏱️ {}ms | {} | {}",
            inspect_start.elapsed().as_millis(),
            url,
            if matches!(verdict, Verdict::Safe) { "✅ Safe".to_string() } else { format!("🚨 {}", verdict_reason(&verdict)) }
        );

        let result = LinkCheckResult {
            url: url.to_string(),
            verdict,
            user_id: None,
            timestamp: Utc::now().timestamp(),
        };

        self.save(&result).await;
        LinkCache::set(url, result.clone(), self.cache_ttl);
        result
    }

    /// حفظ النتيجة في الداتا بيس
    async fn save(&self, result: &LinkCheckResult) {
        if let Some(ref pool) = self.pool {
            let is_safe = result.is_safe();
            let classification = if is_safe { "Safe" } else { "Malicious" };
            let risk_score: f32 = if is_safe { 0.0 } else { 1.0 };
            // آمن = 7 أيام، خطر = 30 يوم
            let ttl = if is_safe { 7 * 24 } else { 30 * 24 };
            if let Err(e) = db_save_url(
                pool,
                &result.url,
                is_safe,
                risk_score,
                classification,
                result.reason(),
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
            println!("[CHECK] url={} safe={} reason={}", url, result.is_safe(), result.reason());

            if !result.is_safe() {
                return Some(Action::Delete);
            }
        }

        None
    }
}

fn verdict_reason(verdict: &Verdict) -> &str {
    match verdict {
        Verdict::Safe => "No threats detected",
        Verdict::Malicious { reason } => reason.as_str(),
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
        _ => {}
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 📝 HELPER FUNCTIONS
// ═══════════════════════════════════════════════════════════════════════

pub fn create_engine(pool: Arc<MySqlPool>) -> LinkSecurityEngine {
    LinkSecurityEngine::new(Some(pool), 24)
}
