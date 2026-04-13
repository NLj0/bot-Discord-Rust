use sqlx::mysql::MySqlPool;
use sqlx::Row;
use chrono::{DateTime, Utc};

/// تهيئة قاعدة البيانات والجداول
pub async fn init_database(database_url: &str) -> Result<MySqlPool, sqlx::Error> {
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

    println!("✅ قاعدة البيانات والجداول تم إنشاؤها بنجاح!");
    Ok(())
}

/// إضافة مستخدم جديد
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
pub struct UserStats {
    pub user_id: u64,
    pub username: String,
    pub total_commands: i32,
    pub warnings: i32,
    pub is_banned: bool,
}
