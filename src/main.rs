use serenity::async_trait;
use serenity::model::application::{Command, CommandOptionType, Interaction};
use serenity::model::gateway::Ready;
use serenity::model::id::GuildId;
use serenity::model::prelude::Message;
use serenity::builder::{CreateCommand, CreateCommandOption, CreateInteractionResponse, CreateInteractionResponseMessage};
use serenity::prelude::*;
use std::env;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

mod commands;
mod database;
mod link_security;

use commands::{handle_ping, handle_clear, handle_whitelist};
use link_security::{LinkSecurityEngine, handle_message};

struct Handler {
    security_engine: LinkSecurityEngine,
    commands_registered: AtomicBool,
}

// كمفتاح لتخزين قاعدة البيانات في TypeMap
pub struct DatabaseKey;

impl TypeMapKey for DatabaseKey {
    type Value = Arc<sqlx::mysql::MySqlPool>;
}

#[async_trait]
impl EventHandler for Handler {
    // ===== معالج الرسائل (فحص الروابط تلقائياً) =====
    async fn message(&self, ctx: Context, msg: Message) {
        // فحص الرسالة وتنفيذ الإجراء المناسب
        handle_message(&ctx, &msg, &self.security_engine).await;
    }

    // ===== معالج الأوامر =====
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::Command(command) = interaction {
            let user_id = command.user.id.get();

            // الحصول على قاعدة البيانات
            let pool = ctx.data.read().await.get::<DatabaseKey>().cloned();

            // التحقق من حالة الحظر
            let is_banned = if let Some(pool) = &pool {
                database::is_user_banned(&**pool, user_id)
                    .await
                    .unwrap_or(false)
            } else {
                false
            };

            if is_banned {
                let _ = command.create_response(&ctx.http,
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .content("❌ أنت محظور من استخدام هذا البوت")
                    )
                ).await;
                return;
            }

            // تنفيذ الأمر
            match command.data.name.as_str() {
                "ping"      => handle_ping(&ctx, &command).await,
                "clear"     => handle_clear(&ctx, &command).await,
                "whitelist" => handle_whitelist(&ctx, &command).await,
                _ => {
                    let _ = command.create_response(&ctx.http,
                        CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new().content("Unknown command")
                        )
                    ).await;
                }
            }

            // تسجيل استخدام الأمر
            if let Some(pool) = pool {
                let _ = database::log_command(&*pool, user_id, &command.data.name).await;
            }
        }
    }

    // ===== تسجيل الأوامر عند الاتصال =====
    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
        println!("🛡️ Link Security System: ACTIVE");

        // تسجيل الأوامر مرة واحدة فقط حتى عند إعادة الاتصال
        if self.commands_registered.swap(true, Ordering::SeqCst) {
            println!("⏭️ Commands already registered, skipping.");
            return;
        }

        // إذا وُجد GUILD_ID → guild commands (تنتشر فورياً، مثالي للتطوير)
        // إذا لم يوجد → global commands (تحتاج حتى ساعة للانتشار، للإنتاج)
        let guild_id = env::var("GUILD_ID")
            .ok()
            .and_then(|id| id.parse::<u64>().ok())
            .map(GuildId::new);

        let commands = vec![
            CreateCommand::new("ping")
                .description("استجابة البيينج مع قياس السرعة"),
            CreateCommand::new("clear")
                .description("حذف الرسائل من القناة")
                .add_option(
                    CreateCommandOption::new(CommandOptionType::Integer, "amount", "عدد الرسائل المراد حذفها (1-100)")
                        .min_int_value(1)
                        .max_int_value(100)
                        .required(false)
                ),
            CreateCommand::new("whitelist")
                .description("إدارة القائمة البيضاء للروابط")
                .add_option(
                    CreateCommandOption::new(CommandOptionType::SubCommand, "add", "إضافة رابط أو domain للقائمة البيضاء")
                        .add_sub_option(
                            CreateCommandOption::new(CommandOptionType::String, "domain", "الرابط أو الـ domain مثال: youtube.com")
                                .required(true)
                        )
                )
                .add_option(
                    CreateCommandOption::new(CommandOptionType::SubCommand, "remove", "حذف domain من القائمة البيضاء")
                        .add_sub_option(
                            CreateCommandOption::new(CommandOptionType::String, "domain", "الـ domain المراد حذفه")
                                .required(true)
                        )
                ),
        ];

        match guild_id {
            Some(gid) => {
                match gid.set_commands(&ctx.http, commands).await {
                    Ok(cmds) => println!("✅ {} أوامر مسجّلة على السيرفر (فورياً)", cmds.len()),
                    Err(e)   => println!("❌ خطأ في تسجيل أوامر السيرفر: {:?}", e),
                }
            }
            None => {
                match Command::set_global_commands(&ctx.http, commands).await {
                    Ok(cmds) => println!("✅ {} أوامر مسجّلة عالمياً (قد تحتاج حتى ساعة)", cmds.len()),
                    Err(e)   => println!("❌ خطأ في تسجيل الأوامر العالمية: {:?}", e),
                }
            }
        }
    }
}

#[tokio::main]
async fn main() {
    // Load environment variables
    dotenv::dotenv().ok();
    
    let token = env::var("DISCORD_TOKEN")
        .expect("Expected a token in the environment");

    let database_url = env::var("DATABASE_URL")
        .expect("Expected DATABASE_URL in the environment");

    // إعدادات نظام الحماية
    let google_api_key = env::var("GOOGLE_SAFE_BROWSING_API_KEY").ok();
    let virustotal_api_key = env::var("VIRUSTOTAL_API_KEY").ok();

    // تهيئة قاعدة البيانات
    let pool = match database::init_database(&database_url).await {
        Ok(pool) => {
            println!("✅ تم الاتصال بقاعدة البيانات بنجاح!");
            Arc::new(pool)
        }
        Err(e) => {
            eprintln!("❌ خطأ في الاتصال بقاعدة البيانات: {}", e);
            return;
        }
    };

    // إنشاء محرك الحماية
    let security_engine = link_security::create_engine(google_api_key, virustotal_api_key, Arc::clone(&pool));

    // نسخة للـ graceful shutdown بعد توقف الـ client
    let pool_for_shutdown = Arc::clone(&pool);

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::GUILDS
        | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(&token, intents)
        .event_handler(Handler {
            security_engine,
            commands_registered: AtomicBool::new(false),
        })
        .await
        .expect("Error creating client");

    // إضافة قاعدة البيانات
    {
        let mut data = client.data.write().await;
        data.insert::<DatabaseKey>(pool);
    }

    if let Err(why) = client.start().await {
        eprintln!("Client error: {:?}", why);
    }

    // Graceful shutdown: إغلاق اتصالات قاعدة البيانات بأمان
    println!("🔌 Closing database connections...");
    pool_for_shutdown.close().await;
    println!("✅ Database connections closed.");
}