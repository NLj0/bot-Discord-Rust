use serenity::async_trait;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use std::env;

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    // معالج الرسائل - يطبع كل رسالة في الترمينال
    async fn message(&self, _ctx: Context, msg: Message) {
        // تجاهل رسائل البوتات
        if msg.author.bot {
            return;
        }

        // تنظيف النص من الإيموجي والصور
        let content = clean_message_content(&msg);
        
        // إذا كان النص فاضي بعد التنظيف، تجاهله
        if content.is_empty() {
            return;
        }

        // طباعة المعلومات في الترمينال
        println!("\n============================================================");
        println!("📨 رسالة جديدة");
        println!("============================================================");
        println!("👤 المرسل: {} (ID: {})", msg.author.name, msg.author.id);
        println!("📍 القناة: {} (ID: {})", 
            msg.channel_id,
            msg.channel_id
        );
        
        // معلومات السيرفر
        if let Some(guild_id) = msg.guild_id {
            println!("🏠 السيرفر ID: {}", guild_id);
        }

        // معلومات الرد (Reply)
        if let Some(referenced) = &msg.referenced_message {
            println!("↩️  رد على: {} (ID: {})", 
                referenced.author.name,
                referenced.author.id
            );
            let ref_content = clean_text(&referenced.content);
            if !ref_content.is_empty() {
                println!("   💬 الرسالة الأصلية: \"{}\"", 
                    truncate(&ref_content, 100)
                );
            }
        }

        // الوقت
        println!("🕐 الوقت: {}", msg.timestamp);

        // المحتوى
        println!("💬 المحتوى:");
        println!("   \"{}\"", content);

        // المرفقات (صور، ملفات)
        if !msg.attachments.is_empty() {
            println!("📎 المرفقات ({}):", msg.attachments.len());
            for (i, attachment) in msg.attachments.iter().enumerate() {
                let content_type = attachment.content_type.as_deref().unwrap_or("unknown");
                println!("   {}. {} ({})", i + 1, attachment.filename, content_type);
            }
        }

        // التفاعلات (Reactions)
        if !msg.reactions.is_empty() {
            println!("❤️  التفاعلات:");
            for reaction in &msg.reactions {
                println!("   {} x{}", reaction.reaction_type, reaction.count);
            }
        }

        // Embeds (لو موجودة)
        if !msg.embeds.is_empty() {
            println!("📰 Embeds: {} موجود", msg.embeds.len());
        }

        println!("============================================================\n");
    }

    async fn ready(&self, _ctx: Context, ready: Ready) {
        println!("\n🚀 ============================================================");
        println!("✅ Selfbot متصل بنجاح!");
        println!("👤 الحساب: {}", ready.user.name);
        println!("🆔 ID: {}", ready.user.id);
        println!("📡 جاهز لقراءة الرسائل...");
        println!("============================================================\n");
    }
}

// دالة لتنظيف محتوى الرسالة
fn clean_message_content(msg: &Message) -> String {
    let mut content = msg.content.clone();
    
    // حذف mentions
    content = content.replace("<@!", "").replace("<@", "").replace(">", "");
    
    // تنظيف النص
    clean_text(&content)
}

// دالة لتنظيف النص من الإيموجي والرموز غير المرغوبة
fn clean_text(text: &str) -> String {
    text.chars()
        .filter(|c| {
            // احتفظ بـ:
            // - الأحرف العربية (U+0600 to U+06FF)
            // - الأحرف الإنجليزية (a-z, A-Z)
            // - الأرقام (0-9)
            // - المسافات والترقيم الأساسي
            matches!(c,
                '\u{0600}'..='\u{06FF}' |  // عربي
                'a'..='z' | 'A'..='Z' |     // إنجليزي
                '0'..='9' |                 // أرقام
                ' ' | '.' | ',' | '!' | '?' | ':' | ';' | '-' | '_' | '\n'
            )
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

// دالة لاختصار النص الطويل
fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        s.chars().take(max_chars).collect::<String>() + "..."
    }
}

#[tokio::main]
async fn main() {
    // تحميل متغيرات البيئة
    dotenv::dotenv().ok();

    println!("\n🔧 ============================================================");
    println!("⚙️  تهيئة Selfbot...");
    println!("============================================================");

    // الحصول على User Token (ليس Bot Token!)
    let token = env::var("USER_TOKEN")
        .expect("❌ خطأ: USER_TOKEN غير موجود في .env");

    println!("✅ Token تم تحميله");

    // إعدادات الـ Gateway Intents
    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::GUILDS
        | GatewayIntents::MESSAGE_CONTENT;

    println!("✅ Intents تم تهيئتها");

    // إنشاء الـ Client
    let mut client = Client::builder(&token, intents)
        .event_handler(Handler)
        .await
        .expect("❌ خطأ في إنشاء Client");

    println!("✅ Client جاهز");
    println!("🔄 جاري الاتصال بـ Discord...\n");

    // تشغيل الـ Client
    if let Err(why) = client.start().await {
        eprintln!("\n❌ خطأ في الاتصال: {:?}", why);
        eprintln!("\n⚠️  تحقق من:");
        eprintln!("   1. USER_TOKEN صحيح");
        eprintln!("   2. الحساب غير محظور");
        eprintln!("   3. الاتصال بالإنترنت");
    }
}
