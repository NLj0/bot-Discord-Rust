use sqlx::mysql::MySqlPool;
use sqlx::Row;

/// تهيئة قاعدة البيانات والجداول
pub async fn init_database(database_url: &str) -> Result<MySqlPool, sqlx::Error> {
    // استخراج اسم قاعدة البيانات من الـ URL وإنشاؤها إن لم تكن موجودة
    if let Some(slash_pos) = database_url.rfind('/') {
        let base_url = &database_url[..slash_pos];
        let db_name = &database_url[slash_pos + 1..];

        // الاتصال بـ MySQL بدون تحديد قاعدة بيانات
        if let Ok(base_pool) = MySqlPool::connect(base_url).await {
            let create_sql = format!("CREATE DATABASE IF NOT EXISTS `{}`", db_name);
            let _ = sqlx::query(&create_sql).execute(&base_pool).await;
            base_pool.close().await;
        }
    }

    let pool = MySqlPool::connect(database_url).await?;
    
    // إنشء جداول قاعدة البيانات
    create_tables(&pool).await?;
    
    Ok(pool)
}

/// إنشاء الجداول المطلوبة
async fn create_tables(pool: &MySqlPool) -> Result<(), sqlx::Error> {
    // جدول المستخدمين
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            user_id BIGINT PRIMARY KEY,
            username VARCHAR(255) NOT NULL,
            join_date TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            total_commands INTEGER DEFAULT 0,
            warnings INTEGER DEFAULT 0,
            is_banned BOOLEAN DEFAULT FALSE,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )
        "#
    )
    .execute(pool)
    .await?;

    // جدول سجل الأوامر
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS command_logs (
            id INT AUTO_INCREMENT PRIMARY KEY,
            user_id BIGINT NOT NULL,
            command_name VARCHAR(100) NOT NULL,
            executed_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            status VARCHAR(50) DEFAULT 'success',
            FOREIGN KEY (user_id) REFERENCES users(user_id)
        )
        "#
    )
    .execute(pool)
    .await?;

    // جدول الإحصائيات
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS statistics (
            id INT AUTO_INCREMENT PRIMARY KEY,
            user_id BIGINT NOT NULL,
            command_name VARCHAR(100) NOT NULL,
            usage_count INTEGER DEFAULT 0,
            last_used TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (user_id) REFERENCES users(user_id)
        )
        "#
    )
    .execute(pool)
    .await?;

    // جدول التحذيرات والعقوبات
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS moderation (
            id INT AUTO_INCREMENT PRIMARY KEY,
            user_id BIGINT NOT NULL,
            reason VARCHAR(500),
            warning_type VARCHAR(50),
            moderator_id BIGINT,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (user_id) REFERENCES users(user_id)
        )
        "#
    )
    .execute(pool)
    .await?;

    // جدول كاش الروابط
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS url_cache (
            url_hash CHAR(32) PRIMARY KEY,
            url VARCHAR(2048) NOT NULL,
            is_safe BOOLEAN NOT NULL,
            risk_score FLOAT NOT NULL,
            classification VARCHAR(50) NOT NULL,
            reason VARCHAR(500) NOT NULL,
            checked_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            expires_at TIMESTAMP NOT NULL
        )
        "#
    )
    .execute(pool)
    .await?;

    // جدول القائمة البيضاء
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS whitelist (
            domain VARCHAR(255) PRIMARY KEY,
            added_by BIGINT NOT NULL,
            added_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )
        "#
    )
    .execute(pool)
    .await?;

    println!("✅ قاعدة البيانات والجداول تم إنشاؤها بنجاح!");
    Ok(())
}

/// التحقق إذا domain في الـ whitelist
pub async fn is_whitelisted(pool: &MySqlPool, domain: &str) -> bool {
    sqlx::query("SELECT 1 FROM whitelist WHERE domain = ?")
        .bind(domain)
        .fetch_optional(pool)
        .await
        .map(|r| r.is_some())
        .unwrap_or(false)
}

/// إضافة domain للـ whitelist
pub async fn add_to_whitelist(
    pool: &MySqlPool,
    domain: &str,
    added_by: u64,
) -> Result<bool, sqlx::Error> {
    let affected = sqlx::query(
        "INSERT IGNORE INTO whitelist (domain, added_by) VALUES (?, ?)"
    )
    .bind(domain)
    .bind(added_by as i64)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(affected > 0)
}

/// حذف domain من الـ whitelist
#[allow(dead_code)]
pub async fn remove_from_whitelist(
    pool: &MySqlPool,
    domain: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM whitelist WHERE domain = ?")
        .bind(domain)
        .execute(pool)
        .await?;
    Ok(())
}

/// إضافة مستخدم جديد
#[allow(dead_code)]
pub async fn add_user(
    pool: &MySqlPool,
    user_id: u64,
    username: String,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO users (user_id, username) VALUES (?, ?) ON DUPLICATE KEY UPDATE username = ?"
    )
    .bind(user_id as i64)
    .bind(&username)
    .bind(&username)
    .execute(pool)
    .await?;

    Ok(())
}

/// تسجيل استخدام أمر
pub async fn log_command(
    pool: &MySqlPool,
    user_id: u64,
    command_name: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO command_logs (user_id, command_name) VALUES (?, ?)"
    )
    .bind(user_id as i64)
    .bind(command_name)
    .execute(pool)
    .await?;

    // تحديث عدد الأوامر المستخدمة
    sqlx::query(
        "UPDATE users SET total_commands = total_commands + 1 WHERE user_id = ?"
    )
    .bind(user_id as i64)
    .execute(pool)
    .await?;

    Ok(())
}

/// الحصول على إحصائيات مستخدم
#[allow(dead_code)]
pub async fn get_user_stats(
    pool: &MySqlPool,
    user_id: u64,
) -> Result<Option<UserStats>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT user_id, username, total_commands, warnings, is_banned FROM users WHERE user_id = ?"
    )
    .bind(user_id as i64)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| UserStats {
        user_id: r.get::<i64, _>("user_id") as u64,
        username: r.get("username"),
        total_commands: r.get("total_commands"),
        warnings: r.get("warnings"),
        is_banned: r.get("is_banned"),
    }))
}

/// إضافة تحذير للمستخدم
#[allow(dead_code)]
pub async fn add_warning(
    pool: &MySqlPool,
    user_id: u64,
    reason: &str,
    moderator_id: u64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO moderation (user_id, reason, warning_type, moderator_id) VALUES (?, ?, ?, ?)"
    )
    .bind(user_id as i64)
    .bind(reason)
    .bind("warning")
    .bind(moderator_id as i64)
    .execute(pool)
    .await?;

    // تحديث عدد التحذيرات
    sqlx::query(
        "UPDATE users SET warnings = warnings + 1 WHERE user_id = ?"
    )
    .bind(user_id as i64)
    .execute(pool)
    .await?;

    Ok(())
}

/// حظر مستخدم
#[allow(dead_code)]
pub async fn ban_user(
    pool: &MySqlPool,
    user_id: u64,
    reason: &str,
    moderator_id: u64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO moderation (user_id, reason, warning_type, moderator_id) VALUES (?, ?, ?, ?)"
    )
    .bind(user_id as i64)
    .bind(reason)
    .bind("ban")
    .bind(moderator_id as i64)
    .execute(pool)
    .await?;

    // تعديل حالة الحظر
    sqlx::query(
        "UPDATE users SET is_banned = TRUE WHERE user_id = ?"
    )
    .bind(user_id as i64)
    .execute(pool)
    .await?;

    Ok(())
}

/// التحقق من كون المستخدم محظور
pub async fn is_user_banned(
    pool: &MySqlPool,
    user_id: u64,
) -> Result<bool, sqlx::Error> {
    let row = sqlx::query(
        "SELECT is_banned FROM users WHERE user_id = ?"
    )
    .bind(user_id as i64)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| r.get::<bool, _>("is_banned")).unwrap_or(false))
}

/// الحصول على أعلى المستخدمين نشاطاً
#[allow(dead_code)]
pub async fn get_top_users(
    pool: &MySqlPool,
    limit: i64,
) -> Result<Vec<UserStats>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT user_id, username, total_commands, warnings, is_banned FROM users ORDER BY total_commands DESC LIMIT ?"
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| UserStats {
        user_id: r.get::<i64, _>("user_id") as u64,
        username: r.get("username"),
        total_commands: r.get("total_commands"),
        warnings: r.get("warnings"),
        is_banned: r.get("is_banned"),
    }).collect())
}

/// هيكل بيانات إحصائيات المستخدم
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct UserStats {
    pub user_id: u64,
    pub username: String,
    pub total_commands: i32,
    pub warnings: i32,
    pub is_banned: bool,
}

/// هيكل نتيجة الرابط المخزّن في الداتا بيس
#[derive(Debug, Clone)]
pub struct CachedUrl {
    pub is_safe: bool,
    pub risk_score: f32,
    pub classification: String,
    pub reason: String,
}

/// البحث عن رابط في الكاش
pub async fn db_get_url(pool: &MySqlPool, url: &str) -> Option<CachedUrl> {
    let hash = format!("{:x}", md5::compute(url));
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

/// حفظ نتيجة رابط في الكاش (TTL بالساعات)
pub async fn db_save_url(
    pool: &MySqlPool,
    url: &str,
    is_safe: bool,
    risk_score: f32,
    classification: &str,
    reason: &str,
    ttl_hours: i64,
) -> Result<(), sqlx::Error> {
    let hash = format!("{:x}", md5::compute(url));
    sqlx::query(
        "INSERT INTO url_cache (url_hash, url, is_safe, risk_score, classification, reason, expires_at) \
         VALUES (?, ?, ?, ?, ?, ?, DATE_ADD(NOW(), INTERVAL ? HOUR)) \
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
