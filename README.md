# 🛡️ Discord Bot - Shield

بوت Discord متطور مكتوب بـ Rust باستخدام مكتبة Serenity، مع دعم قاعدة بيانات MySQL/MariaDB، وإدارة متقدمة للأوامر والمستخدمين.

## ✨ المميزات الرئيسية

- ✅ **أوامر Slash Commands** - أوامر حديثة وسهلة الاستخدام
- ✅ **قاعدة بيانات MySQL/MariaDB** - تخزين البيانات والإحصائيات
- ✅ **نظام الإدارة** - حذف الرسائل، التحذيرات، والحظر
- ✅ **قياس السرعة** - قياس الـ Ping والكود والـ API
- ✅ **إحصائيات مستخدمين** - تتبع استخدام الأوامر والنشاط
- ✅ **كود محسّن** - معايير Rust الحديثة وأفضل الممارسات

---

## 🔧 المتطلبات

### البرامج المطلوبة
- **Rust**: إصدار 1.70 أو أحدث ([تحميل](https://rustup.rs/))
- **MySQL/MariaDB**: لقاعدة البيانات ([تحميل](https://mariadb.org/download/))
- **Node.js** (اختياري): لأدوات التطوير

### بيانات Discord
- حساب Discord
- Discord Server للاختبار
- [Discord Developer Portal](https://discord.com/developers/applications) - للحصول على:
  - Bot Token
  - Application ID

---

## 📦 خطوات التثبيت

### 1️⃣ استنساخ المشروع
```bash
cd Desktop
git clone <repository-url>
cd "bot Discord Rust"
```

### 2️⃣ تثبيت Rust (إذا لم يكن مثبتاً)
```bash
# Windows
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# أو من الموقع الرسمي: https://rustup.rs/
```

### 3️⃣ إعداد قاعدة البيانات

#### إنشاء قاعدة البيانات
```sql
CREATE DATABASE discord_bot_shield_db CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;
```

#### تأكد من بيانات الاتصال
```
Host: 127.0.0.1 (أو localhost)
Port: 3306 (الافتراضي)
Username: root
Password: <your-password>
Database: discord_bot_shield_db
```

### 4️⃣ تكوين متغيرات البيئة

#### أنشئ ملف `.env` في جذر المشروع
```bash
# Discord Bot Token (من Discord Developer Portal)
DISCORD_TOKEN=<your-bot-token>

# Application ID (من Discord Developer Portal)
APPLICATION_ID=<your-application-id>

# Database Configuration (MariaDB)
DATABASE_URL=mysql://root:<your-password>@127.0.0.1:3306/discord_bot_shield_db

# Logging
RUST_LOG=debug
```

⚠️ **تحذير أمان**: لا تشارك هذه الملفات العامة مع أحد!

### 5️⃣ تثبيت المتطلبات وبناء المشروع
```bash
# تحديث Cargo
cargo update

# بناء المشروع
cargo build --release

# أو للتطوير مع إعادة التحميل التلقائي
cargo build
```

### 6️⃣ تشغيل البوت
```bash
# البناء والتشغيل
cargo run

# أو التشغيل المباشر
./target/release/discord_bot  # على Windows: discord_bot.exe
```

---

## 🚀 الاستخدام

### تشغيل البوت
```bash
cargo run
```

### النتيجة المتوقعة
```
Shield is connected!
✅ Ping command registered!
✅ Clear command registered!
✅ تم الاتصال بقاعدة البيانات بنجاح!
```

### أوامر البوت المتاحة

#### 1. `/ping` - قياس السرعة
```
استخدام: /ping
الوصف: قياس سرعة الكود والـ API
المجموع: الوقت الكلي للاستجابة
```

مثال الرد:
```
🏓 Pong!
⚡ سرعة الكود: 2ms
📡 سرعة API: 3ms
🔄 المجموع: 5ms
```

#### 2. `/clear` - حذف الرسائل
```
استخدام: /clear [عدد الرسائل]
الوصف: حذف الرسائل من القناة (بحد أقصى 100)
الخيار: amount (اختياري، الافتراضي: 5)
الصلاحيات: تحتاج صلاحية "إدارة الرسائل"
```

مثال الاستخدام:
```
/clear amount:10  # يحذف آخر 10 رسائل
/clear            # يحذف آخر 5 رسائل (الافتراضي)
```

---

## 📁 هيكل المشروع

```
bot Discord Rust/
├── src/
│   ├── main.rs              # نقطة الدخول الرئيسية
│   ├── database.rs          # التعامل مع قاعدة البيانات
│   └── commands/
│       ├── mod.rs           # تصدير الأوامر
│       ├── ping.rs          # أمر /ping
│       └── clear.rs         # أمر /clear
├── Cargo.toml              # المتطلبات والإعدادات
├── .env                    # متغيرات البيئة
└── README.md              # هذا الملف
```

---

## 🗄️ هيكل قاعدة البيانات

### جدول `users`
```sql
CREATE TABLE users (
    user_id BIGINT PRIMARY KEY,
    username VARCHAR(255),
    join_date TIMESTAMP,
    total_commands INTEGER,
    warnings INTEGER,
    is_banned BOOLEAN,
    created_at TIMESTAMP
);
```

### جدول `command_logs`
```sql
CREATE TABLE command_logs (
    id INT AUTO_INCREMENT PRIMARY KEY,
    user_id BIGINT,
    command_name VARCHAR(100),
    executed_at TIMESTAMP,
    status VARCHAR(50)
);
```

### جدول `statistics`
```sql
CREATE TABLE statistics (
    id INT AUTO_INCREMENT PRIMARY KEY,
    user_id BIGINT,
    command_name VARCHAR(100),
    usage_count INTEGER,
    last_used TIMESTAMP
);
```

### جدول `moderation`
```sql
CREATE TABLE moderation (
    id INT AUTO_INCREMENT PRIMARY KEY,
    user_id BIGINT,
    reason VARCHAR(500),
    warning_type VARCHAR(50),
    moderator_id BIGINT,
    created_at TIMESTAMP
);
```

---

## 🔐 متغيرات البيئة

| المتغير | الوصف | مثال |
|---------|-------|------|
| `DISCORD_TOKEN` | توكن البوت من Discord | `MTQ5MzA...` |
| `APPLICATION_ID` | معرف التطبيق | `1493017908664733856` |
| `DATABASE_URL` | عنوان قاعدة البيانات | `mysql://root:pass@localhost:3306/db` |
| `RUST_LOG` | مستوى التسجيل | `debug`, `info`, `warn`, `error` |

---

## 📋 المتطلبات (Dependencies)

```toml
serenity = "0.10"          # مكتبة Discord
tokio = "1.35"             # Async runtime
sqlx = "0.7"               # SQL query builder
dotenv = "0.15"            # متغيرات البيئة
time = "0.3.36"            # معالجة الوقت
chrono = "0.4"             # تاريخ ووقت
```

---

## 🐛 استكشاف الأخطاء

### الخطأ: "Unknown command"
**السبب**: الأمر لم يتم تسجيله بعد  
**الحل**: انتظر 30 ثانية وأعد التحميل

### الخطأ: "Database connection failed"
**السبب**: عدم الاتصال بـ MySQL  
**الحل**: 
- تأكد من تشغيل MySQL/MariaDB
- تحقق من بيانات الاتصال في `.env`
- تأكد من إنشاء قاعدة البيانات

### الخطأ: "Invalid bot token"
**السبب**: البوت توكن صحيح  
**الحل**: أعد إنشاء التوكن من Discord Developer Portal

---

## 🛠️ التطوير

### إضافة أمر جديد

#### 1. أنشئ ملف جديد في `src/commands/`
```rust
// src/commands/mycommand.rs
use serenity::client::Context;
use serenity::model::prelude::application_command::ApplicationCommandInteraction;
use serenity::model::interactions::InteractionResponseType;

pub async fn handle_mycommand(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
) {
    let _ = command
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message| {
                    message.content("رد الأمر الجديد")
                })
        })
        .await;
}
```

#### 2. استورده في `src/commands/mod.rs`
```rust
pub mod mycommand;
pub use mycommand::handle_mycommand;
```

#### 3. أضفه في `main.rs`
```rust
"mycommand" => {
    handle_mycommand(&ctx, &command).await;
}
```

---

## 📖 مراجع مفيدة

- [Serenity Documentation](https://docs.rs/serenity/)
- [Discord Developer Documentation](https://discord.com/developers/docs)
- [SQLx Documentation](https://docs.rs/sqlx/)
- [Rust Book](https://doc.rust-lang.org/book/)

---

## 📝 الترخيص

هذا المشروع مرخص تحت **MIT License**

---

## 👨‍💻 المساهمة

نرحب بالمساهمات! يرجى:
1. فورك المشروع
2. أنشئ فرع للميزة الجديدة
3. أرسل Pull Request

---

## ⚠️ تحذيرات أمان مهمة

🔴 **لا تشارك بيانات البوت:**
- ❌ Bot Token
- ❌ DATABASE_URL مع كلمة المرور
- ❌ Application ID

📌 **في حالة التسريب:**
1. اذهب إلى [Discord Developer Portal](https://discord.com/developers/applications)
2. اختر تطبيقك
3. اضغط "Reset Token"
4. عدّل `.env` بـ التوكن الجديد

---

## 📞 الدعم

في حالة وجود مشاكل:
1. تحقق من README
2. راجع السجلات (logs)
3. تأكد من إصدار Rust

---

**آخر تحديث**: 2026-04-13  
**الحالة**: ✅ نشط وجاهز للاستخدام
