use sqlx::mysql::MySqlPool;
use sqlx::Row;
use sha2::{Sha256, Digest};

// ================================================================
// تهيئة قاعدة البيانات
// ================================================================

pub async fn init_database(database_url: &str) -> Result<MySqlPool, sqlx::Error> {
    if let Some(slash_pos) = database_url.rfind('/') {
        let base_url = &database_url[..slash_pos];
        let db_name = &database_url[slash_pos + 1..];

        let is_valid = !db_name.is_empty()
            && db_name.len() <= 64
            && db_name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');

        if !is_valid {
            return Err(sqlx::Error::Configuration(
                format!("اسم قاعدة البيانات غير صالح: '{}'", db_name).into()
            ));
        }

        if let Ok(base_pool) = MySqlPool::connect(base_url).await {
            let create_sql = format!("CREATE DATABASE IF NOT EXISTS `{}`", db_name);
            let _ = sqlx::query(&create_sql).execute(&base_pool).await;
            base_pool.close().await;
        }
    }

    let pool = MySqlPool::connect(database_url).await?;
    create_tables(&pool).await?;
    Ok(pool)
}

// ================================================================
// إنشاء الجداول
// ================================================================

async fn create_tables(pool: &MySqlPool) -> Result<(), sqlx::Error> {

    // 1️⃣ جدول المستخدمين
    sqlx::query(r#"
        CREATE TABLE IF NOT EXISTS users (
            user_id         BIGINT PRIMARY KEY,
            username        VARCHAR(255) NOT NULL,
            join_date       TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            total_commands  INT DEFAULT 0,
            warnings_count  INT DEFAULT 0,
            is_blacklisted  BOOLEAN DEFAULT FALSE,
            blacklist_reason VARCHAR(500),
            blacklist_date  TIMESTAMP NULL,
            created_at      TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            updated_at      TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
            INDEX idx_is_blacklisted (is_blacklisted)
        )
    "#).execute(pool).await?;

    // 2️⃣ جدول سجل الأوامر
    sqlx::query(r#"
        CREATE TABLE IF NOT EXISTS command_logs (
            id           INT AUTO_INCREMENT PRIMARY KEY,
            user_id      BIGINT NOT NULL,
            guild_id     BIGINT NOT NULL DEFAULT 0,
            command_name VARCHAR(100) NOT NULL,
            executed_at  TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            status       VARCHAR(50) DEFAULT 'success',
            INDEX idx_user_guild (user_id, guild_id),
            INDEX idx_executed_at (executed_at)
        )
    "#).execute(pool).await?;

    // 3️⃣ جدول الإدارة والتحذيرات
    sqlx::query(r#"
        CREATE TABLE IF NOT EXISTS moderation (
            id             INT AUTO_INCREMENT PRIMARY KEY,
            guild_id       BIGINT NOT NULL DEFAULT 0,
            user_id        BIGINT NOT NULL,
            action_type    VARCHAR(50) NOT NULL,
            reason         VARCHAR(500) NOT NULL,
            moderator_id   BIGINT NOT NULL DEFAULT 0,
            is_active      BOOLEAN DEFAULT TRUE,
            duration_hours INT NULL,
            expires_at     TIMESTAMP NULL,
            created_at     TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            INDEX idx_guild_user (guild_id, user_id),
            INDEX idx_action_type (action_type),
            INDEX idx_expires_at (expires_at)
        )
    "#).execute(pool).await?;

    // 4️⃣ جدول القائمة البيضاء للروابط
    sqlx::query(r#"
        CREATE TABLE IF NOT EXISTS whitelist_urls (
            id        INT AUTO_INCREMENT PRIMARY KEY,
            domain    VARCHAR(255) NOT NULL UNIQUE,
            guild_id  BIGINT NOT NULL DEFAULT 0,
            added_by  BIGINT NOT NULL,
            added_at  TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            is_active BOOLEAN DEFAULT TRUE,
            INDEX idx_domain (domain)
        )
    "#).execute(pool).await?;

    // 5️⃣ جدول كاش الروابط
    sqlx::query(r#"
        CREATE TABLE IF NOT EXISTS url_cache (
            url_hash       CHAR(64) PRIMARY KEY,
            url            VARCHAR(2048) NOT NULL,
            is_safe        BOOLEAN NOT NULL,
            risk_score     FLOAT NOT NULL,
            classification VARCHAR(50) NOT NULL,
            reason         VARCHAR(500) NOT NULL,
            checked_count  INT NOT NULL DEFAULT 1,
            checked_at     TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            expires_at     TIMESTAMP NOT NULL,
            INDEX idx_expires_at (expires_at)
        )
    "#).execute(pool).await?;

    // 6️⃣ جدول القائمة السوداء للسيرفرات
    sqlx::query(r#"
        CREATE TABLE IF NOT EXISTS server_blacklist (
            id          INT AUTO_INCREMENT PRIMARY KEY,
            server_id   BIGINT NOT NULL UNIQUE,
            server_name VARCHAR(255),
            reason      VARCHAR(500) NOT NULL,
            blocked_by  BIGINT NOT NULL DEFAULT 0,
            is_active   BOOLEAN DEFAULT TRUE,
            created_at  TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            INDEX idx_server_active (server_id, is_active)
        )
    "#).execute(pool).await?;

    // 7️⃣ جدول سجل الإجراءات العام
    sqlx::query(r#"
        CREATE TABLE IF NOT EXISTS audit_logs (
            id         INT AUTO_INCREMENT PRIMARY KEY,
            guild_id   BIGINT NOT NULL DEFAULT 0,
            action     VARCHAR(100) NOT NULL,
            target_id  BIGINT,
            actor_id   BIGINT,
            details    TEXT,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            INDEX idx_guild_action (guild_id, action),
            INDEX idx_created_at (created_at)
        )
    "#).execute(pool).await?;

    // ترقيات على جداول موجودة (تُهمَل إن فشلت = عمود موجود مسبقاً)
    let _ = sqlx::query("ALTER TABLE url_cache MODIFY COLUMN url_hash CHAR(64) NOT NULL").execute(pool).await;
    let _ = sqlx::query("ALTER TABLE url_cache ADD COLUMN checked_count INT NOT NULL DEFAULT 1").execute(pool).await;

    println!("✅ Database and tables created successfully!");
    Ok(())
}

// ================================================================
// القائمة البيضاء - Whitelist
// ================================================================

pub async fn is_whitelisted(pool: &MySqlPool, domain: &str) -> bool {
    sqlx::query("SELECT 1 FROM whitelist_urls WHERE domain = ? AND is_active = TRUE")
        .bind(domain)
        .fetch_optional(pool)
        .await
        .map(|r| r.is_some())
        .unwrap_or(false)
}

pub async fn add_to_whitelist(
    pool: &MySqlPool,
    domain: &str,
    added_by: u64,
    guild_id: u64,
) -> Result<bool, sqlx::Error> {
    let affected = sqlx::query(
        "INSERT INTO whitelist_urls (domain, added_by, guild_id) VALUES (?, ?, ?) \
         ON DUPLICATE KEY UPDATE is_active = TRUE, added_by = VALUES(added_by)"
    )
    .bind(domain)
    .bind(added_by as i64)
    .bind(guild_id as i64)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(affected > 0)
}

pub async fn remove_from_whitelist(
    pool: &MySqlPool,
    domain: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE whitelist_urls SET is_active = FALSE WHERE domain = ?")
        .bind(domain)
        .execute(pool)
        .await?;
    Ok(())
}

// ================================================================
// المستخدمون - Users
// ================================================================

pub async fn ensure_user(
    pool: &MySqlPool,
    user_id: u64,
    username: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO users (user_id, username) VALUES (?, ?) \
         ON DUPLICATE KEY UPDATE username = VALUES(username)"
    )
    .bind(user_id as i64)
    .bind(username)
    .execute(pool)
    .await?;
    Ok(())
}

#[allow(dead_code)]
pub async fn add_user(
    pool: &MySqlPool,
    user_id: u64,
    username: String,
) -> Result<(), sqlx::Error> {
    ensure_user(pool, user_id, &username).await
}

pub async fn is_user_blacklisted(
    pool: &MySqlPool,
    user_id: u64,
) -> Result<bool, sqlx::Error> {
    let row = sqlx::query("SELECT is_blacklisted FROM users WHERE user_id = ?")
        .bind(user_id as i64)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.get::<bool, _>("is_blacklisted")).unwrap_or(false))
}

/// دالة قديمة للتوافق
pub async fn is_user_banned(pool: &MySqlPool, user_id: u64) -> Result<bool, sqlx::Error> {
    is_user_blacklisted(pool, user_id).await
}

#[allow(dead_code)]
pub async fn add_user_to_blacklist(
    pool: &MySqlPool,
    user_id: u64,
    reason: &str,
    moderator_id: u64,
    guild_id: u64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE users SET is_blacklisted = TRUE, blacklist_reason = ?, blacklist_date = NOW() \
         WHERE user_id = ?"
    )
    .bind(reason)
    .bind(user_id as i64)
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT INTO moderation (guild_id, user_id, action_type, reason, moderator_id) \
         VALUES (?, ?, 'blacklist', ?, ?)"
    )
    .bind(guild_id as i64)
    .bind(user_id as i64)
    .bind(reason)
    .bind(moderator_id as i64)
    .execute(pool)
    .await?;

    Ok(())
}

#[allow(dead_code)]
pub async fn remove_user_from_blacklist(
    pool: &MySqlPool,
    user_id: u64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE users SET is_blacklisted = FALSE, blacklist_reason = NULL, blacklist_date = NULL \
         WHERE user_id = ?"
    )
    .bind(user_id as i64)
    .execute(pool)
    .await?;
    Ok(())
}

#[allow(dead_code)]
pub async fn add_warning(
    pool: &MySqlPool,
    user_id: u64,
    reason: &str,
    moderator_id: u64,
    guild_id: u64,
) -> Result<i32, sqlx::Error> {
    sqlx::query(
        "INSERT INTO moderation (guild_id, user_id, action_type, reason, moderator_id) \
         VALUES (?, ?, 'warning', ?, ?)"
    )
    .bind(guild_id as i64)
    .bind(user_id as i64)
    .bind(reason)
    .bind(moderator_id as i64)
    .execute(pool)
    .await?;

    sqlx::query("UPDATE users SET warnings_count = warnings_count + 1 WHERE user_id = ?")
        .bind(user_id as i64)
        .execute(pool)
        .await?;

    let row = sqlx::query("SELECT warnings_count FROM users WHERE user_id = ?")
        .bind(user_id as i64)
        .fetch_optional(pool)
        .await?;

    Ok(row.map(|r| r.get::<i32, _>("warnings_count")).unwrap_or(0))
}

#[allow(dead_code)]
pub async fn get_user_warnings(
    pool: &MySqlPool,
    user_id: u64,
) -> Result<i32, sqlx::Error> {
    let row = sqlx::query("SELECT warnings_count FROM users WHERE user_id = ?")
        .bind(user_id as i64)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.get::<i32, _>("warnings_count")).unwrap_or(0))
}

#[allow(dead_code)]
pub async fn get_user_stats(
    pool: &MySqlPool,
    user_id: u64,
) -> Result<Option<UserStats>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT user_id, username, total_commands, warnings_count, is_blacklisted, blacklist_reason \
         FROM users WHERE user_id = ?"
    )
    .bind(user_id as i64)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| UserStats {
        user_id:          r.get::<i64, _>("user_id") as u64,
        username:         r.get("username"),
        total_commands:   r.get("total_commands"),
        warnings_count:   r.get("warnings_count"),
        is_blacklisted:   r.get("is_blacklisted"),
        blacklist_reason: r.get("blacklist_reason"),
    }))
}

#[allow(dead_code)]
pub async fn get_top_users(
    pool: &MySqlPool,
    limit: i64,
) -> Result<Vec<UserStats>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT user_id, username, total_commands, warnings_count, is_blacklisted, blacklist_reason \
         FROM users ORDER BY total_commands DESC LIMIT ?"
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| UserStats {
        user_id:          r.get::<i64, _>("user_id") as u64,
        username:         r.get("username"),
        total_commands:   r.get("total_commands"),
        warnings_count:   r.get("warnings_count"),
        is_blacklisted:   r.get("is_blacklisted"),
        blacklist_reason: r.get("blacklist_reason"),
    }).collect())
}

// ================================================================
// القائمة السوداء للسيرفرات
// ================================================================

#[allow(dead_code)]
pub async fn is_server_blacklisted(
    pool: &MySqlPool,
    server_id: u64,
) -> Result<bool, sqlx::Error> {
    let row = sqlx::query(
        "SELECT 1 FROM server_blacklist WHERE server_id = ? AND is_active = TRUE"
    )
    .bind(server_id as i64)
    .fetch_optional(pool)
    .await?;
    Ok(row.is_some())
}

#[allow(dead_code)]
pub async fn add_server_to_blacklist(
    pool: &MySqlPool,
    server_id: u64,
    server_name: &str,
    reason: &str,
    blocked_by: u64,
) -> Result<bool, sqlx::Error> {
    let affected = sqlx::query(
        "INSERT INTO server_blacklist (server_id, server_name, reason, blocked_by) VALUES (?, ?, ?, ?) \
         ON DUPLICATE KEY UPDATE is_active = TRUE, reason = VALUES(reason), server_name = VALUES(server_name)"
    )
    .bind(server_id as i64)
    .bind(server_name)
    .bind(reason)
    .bind(blocked_by as i64)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(affected > 0)
}

#[allow(dead_code)]
pub async fn remove_server_from_blacklist(
    pool: &MySqlPool,
    server_id: u64,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE server_blacklist SET is_active = FALSE WHERE server_id = ?")
        .bind(server_id as i64)
        .execute(pool)
        .await?;
    Ok(())
}

// ================================================================
// سجل الأوامر
// ================================================================

pub async fn log_command(
    pool: &MySqlPool,
    user_id: u64,
    guild_id: u64,
    command_name: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO command_logs (user_id, guild_id, command_name) VALUES (?, ?, ?)"
    )
    .bind(user_id as i64)
    .bind(guild_id as i64)
    .bind(command_name)
    .execute(pool)
    .await?;

    sqlx::query("UPDATE users SET total_commands = total_commands + 1 WHERE user_id = ?")
        .bind(user_id as i64)
        .execute(pool)
        .await?;

    Ok(())
}

// ================================================================
// سجل الإجراءات العام - Audit Log
// ================================================================

#[allow(dead_code)]
pub async fn add_audit_log(
    pool: &MySqlPool,
    guild_id: u64,
    action: &str,
    target_id: Option<u64>,
    actor_id: Option<u64>,
    details: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO audit_logs (guild_id, action, target_id, actor_id, details) \
         VALUES (?, ?, ?, ?, ?)"
    )
    .bind(guild_id as i64)
    .bind(action)
    .bind(target_id.map(|id| id as i64))
    .bind(actor_id.map(|id| id as i64))
    .bind(details)
    .execute(pool)
    .await?;
    Ok(())
}

// ================================================================
// الهياكل
// ================================================================

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct UserStats {
    pub user_id:          u64,
    pub username:         String,
    pub total_commands:   i32,
    pub warnings_count:   i32,
    pub is_blacklisted:   bool,
    pub blacklist_reason: Option<String>,
}

// ================================================================
// الهياكل الجديدة
// ================================================================

#[derive(Debug, Clone)]
pub struct ServerEntry {
    pub server_id:   u64,
    pub server_name: String,
    pub reason:      String,
    pub blocked_by:  u64,
    pub created_at:  String,
}

#[derive(Debug, Clone)]
pub struct WarningEntry {
    pub id:           i64,
    pub user_id:      u64,
    pub reason:       String,
    pub moderator_id: u64,
    pub created_at:   String,
}

#[derive(Debug, Clone)]
pub struct BannedUserEntry {
    pub user_id:          u64,
    pub username:         String,
    pub blacklist_reason: Option<String>,
    pub blacklist_date:   Option<String>,
}

// ================================================================
// استعلامات مرقّمة الصفحات
// ================================================================

pub async fn get_blacklisted_servers_paginated(
    pool: &MySqlPool,
    page: i64,
    per_page: i64,
) -> Result<(Vec<ServerEntry>, i64), sqlx::Error> {
    let offset = (page - 1) * per_page;

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM server_blacklist WHERE is_active = TRUE"
    )
    .fetch_one(pool)
    .await?;

    let rows = sqlx::query(
        "SELECT server_id, COALESCE(server_name,'') AS server_name, reason, blocked_by, \
         DATE_FORMAT(created_at, '%Y-%m-%d %H:%i') AS created_at \
         FROM server_blacklist WHERE is_active = TRUE \
         ORDER BY created_at DESC LIMIT ? OFFSET ?"
    )
    .bind(per_page)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    let entries = rows.into_iter().map(|r| ServerEntry {
        server_id:   r.get::<i64, _>("server_id") as u64,
        server_name: r.get("server_name"),
        reason:      r.get("reason"),
        blocked_by:  r.get::<i64, _>("blocked_by") as u64,
        created_at:  r.get("created_at"),
    }).collect();

    Ok((entries, total))
}

pub async fn get_warnings_paginated(
    pool: &MySqlPool,
    user_id: Option<u64>,
    page: i64,
    per_page: i64,
) -> Result<(Vec<WarningEntry>, i64), sqlx::Error> {
    let offset = (page - 1) * per_page;

    let (total, rows) = if let Some(uid) = user_id {
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM moderation WHERE action_type = 'warning' AND user_id = ? AND is_active = TRUE"
        )
        .bind(uid as i64)
        .fetch_one(pool)
        .await?;

        let rows = sqlx::query(
            "SELECT id, user_id, reason, moderator_id, \
             DATE_FORMAT(created_at, '%Y-%m-%d %H:%i') AS created_at \
             FROM moderation WHERE action_type = 'warning' AND user_id = ? AND is_active = TRUE \
             ORDER BY created_at DESC LIMIT ? OFFSET ?"
        )
        .bind(uid as i64)
        .bind(per_page)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        (total, rows)
    } else {
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM moderation WHERE action_type = 'warning' AND is_active = TRUE"
        )
        .fetch_one(pool)
        .await?;

        let rows = sqlx::query(
            "SELECT id, user_id, reason, moderator_id, \
             DATE_FORMAT(created_at, '%Y-%m-%d %H:%i') AS created_at \
             FROM moderation WHERE action_type = 'warning' AND is_active = TRUE \
             ORDER BY created_at DESC LIMIT ? OFFSET ?"
        )
        .bind(per_page)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        (total, rows)
    };

    let entries = rows.into_iter().map(|r| WarningEntry {
        id:           r.get::<i64, _>("id"),
        user_id:      r.get::<i64, _>("user_id") as u64,
        reason:       r.get("reason"),
        moderator_id: r.get::<i64, _>("moderator_id") as u64,
        created_at:   r.get("created_at"),
    }).collect();

    Ok((entries, total))
}

pub async fn remove_last_warning(
    pool: &MySqlPool,
    user_id: u64,
) -> Result<bool, sqlx::Error> {
    // Find latest active warning id
    let row = sqlx::query(
        "SELECT id FROM moderation \
         WHERE action_type = 'warning' AND user_id = ? AND is_active = TRUE \
         ORDER BY created_at DESC LIMIT 1"
    )
    .bind(user_id as i64)
    .fetch_optional(pool)
    .await?;

    let id: i64 = match row {
        Some(r) => r.get("id"),
        None => return Ok(false),
    };

    sqlx::query("UPDATE moderation SET is_active = FALSE WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    sqlx::query("UPDATE users SET warnings_count = GREATEST(warnings_count - 1, 0) WHERE user_id = ?")
        .bind(user_id as i64)
        .execute(pool)
        .await?;

    Ok(true)
}

pub async fn get_banned_users_paginated(
    pool: &MySqlPool,
    page: i64,
    per_page: i64,
) -> Result<(Vec<BannedUserEntry>, i64), sqlx::Error> {
    let offset = (page - 1) * per_page;

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users WHERE is_blacklisted = TRUE"
    )
    .fetch_one(pool)
    .await?;

    let rows = sqlx::query(
        "SELECT user_id, username, blacklist_reason, \
         DATE_FORMAT(blacklist_date, '%Y-%m-%d %H:%i') AS blacklist_date \
         FROM users WHERE is_blacklisted = TRUE \
         ORDER BY blacklist_date DESC LIMIT ? OFFSET ?"
    )
    .bind(per_page)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    let entries = rows.into_iter().map(|r| BannedUserEntry {
        user_id:          r.get::<i64, _>("user_id") as u64,
        username:         r.get("username"),
        blacklist_reason: r.get("blacklist_reason"),
        blacklist_date:   r.get("blacklist_date"),
    }).collect();

    Ok((entries, total))
}

#[derive(Debug, Clone)]
pub struct CachedUrl {
    pub is_safe:        bool,
    pub risk_score:     f32,
    pub classification: String,
    pub reason:         String,
}

// ================================================================
// كاش الروابط - URL Cache
// ================================================================

pub async fn db_get_url(pool: &MySqlPool, url: &str) -> Option<CachedUrl> {
    let hash = format!("{:x}", Sha256::digest(url.as_bytes()));

    // increment checked_count لتتبع الروابط المكررة
    let _ = sqlx::query(
        "UPDATE url_cache SET checked_count = checked_count + 1 \
         WHERE url_hash = ? AND expires_at > NOW()"
    )
    .bind(&hash)
    .execute(pool)
    .await;

    let row = sqlx::query(
        "SELECT is_safe, risk_score, classification, reason FROM url_cache \
         WHERE url_hash = ? AND expires_at > NOW()"
    )
    .bind(&hash)
    .fetch_optional(pool)
    .await
    .ok()?;

    row.map(|r| CachedUrl {
        is_safe:        r.get("is_safe"),
        risk_score:     r.get::<f32, _>("risk_score"),
        classification: r.get("classification"),
        reason:         r.get("reason"),
    })
}

pub async fn db_save_url(
    pool: &MySqlPool,
    url: &str,
    is_safe: bool,
    risk_score: f32,
    classification: &str,
    reason: &str,
    ttl_hours: i64,
) -> Result<(), sqlx::Error> {
    let hash = format!("{:x}", Sha256::digest(url.as_bytes()));
    sqlx::query(
        "INSERT INTO url_cache \
         (url_hash, url, is_safe, risk_score, classification, reason, checked_count, expires_at) \
         VALUES (?, ?, ?, ?, ?, ?, 1, DATE_ADD(NOW(), INTERVAL ? HOUR)) \
         ON DUPLICATE KEY UPDATE \
         is_safe=VALUES(is_safe), risk_score=VALUES(risk_score), \
         classification=VALUES(classification), reason=VALUES(reason), \
         checked_at=NOW(), expires_at=VALUES(expires_at)"
    )
    .bind(&hash)
    .bind(url)
    .bind(is_safe)
    .bind(risk_score)
    .bind(classification)
    .bind(reason)
    .bind(ttl_hours)
    .execute(pool)
    .await?;
    Ok(())
}
