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

use commands::{handle_ping, handle_clear, handle_whitelist, handle_blacklist_server, handle_warn, handle_ban};
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
                "ping"             => handle_ping(&ctx, &command).await,
                "clear"            => handle_clear(&ctx, &command).await,
                "whitelist"        => handle_whitelist(&ctx, &command).await,
                "blacklist-server" => handle_blacklist_server(&ctx, &command).await,
                "warn"             => handle_warn(&ctx, &command).await,
                "ban"              => handle_ban(&ctx, &command).await,
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
                let guild_id = command.guild_id.map(|g| g.get()).unwrap_or(0);
                let _ = database::log_command(&*pool, user_id, guild_id, &command.data.name).await;
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
            CreateCommand::new("blacklist-server")
                .description("Manage server blacklist")
                .add_option(
                    CreateCommandOption::new(CommandOptionType::SubCommand, "add", "Add a server to the blacklist")
                        .add_sub_option(
                            CreateCommandOption::new(CommandOptionType::String, "guild_id", "Server ID (snowflake)")
                                .required(true)
                        )
                        .add_sub_option(
                            CreateCommandOption::new(CommandOptionType::String, "reason", "Reason for blacklisting")
                                .required(false)
                        )
                )
                .add_option(
                    CreateCommandOption::new(CommandOptionType::SubCommand, "remove", "Remove a server from the blacklist")
                        .add_sub_option(
                            CreateCommandOption::new(CommandOptionType::String, "guild_id", "Server ID to remove")
                                .required(true)
                        )
                )
                .add_option(
                    CreateCommandOption::new(CommandOptionType::SubCommand, "list", "List all blacklisted servers")
                        .add_sub_option(
                            CreateCommandOption::new(CommandOptionType::Integer, "page", "Page number")
                                .min_int_value(1)
                                .required(false)
                        )
                ),
            CreateCommand::new("warn")
                .description("Manage user warnings")
                .add_option(
                    CreateCommandOption::new(CommandOptionType::SubCommand, "add", "Warn a user")
                        .add_sub_option(
                            CreateCommandOption::new(CommandOptionType::User, "user", "User to warn")
                                .required(true)
                        )
                        .add_sub_option(
                            CreateCommandOption::new(CommandOptionType::String, "reason", "Reason for warning")
                                .required(false)
                        )
                )
                .add_option(
                    CreateCommandOption::new(CommandOptionType::SubCommand, "remove", "Remove latest warning from a user")
                        .add_sub_option(
                            CreateCommandOption::new(CommandOptionType::User, "user", "User to unwarn")
                                .required(true)
                        )
                )
                .add_option(
                    CreateCommandOption::new(CommandOptionType::SubCommand, "list", "List warnings")
                        .add_sub_option(
                            CreateCommandOption::new(CommandOptionType::User, "user", "Filter by user (optional)")
                                .required(false)
                        )
                        .add_sub_option(
                            CreateCommandOption::new(CommandOptionType::Integer, "page", "Page number")
                                .min_int_value(1)
                                .required(false)
                        )
                ),
            CreateCommand::new("ban")
                .description("Manage bot bans")
                .add_option(
                    CreateCommandOption::new(CommandOptionType::SubCommand, "add", "Ban a user from using the bot")
                        .add_sub_option(
                            CreateCommandOption::new(CommandOptionType::User, "user", "User to ban")
                                .required(true)
                        )
                        .add_sub_option(
                            CreateCommandOption::new(CommandOptionType::String, "reason", "Reason for ban")
                                .required(false)
                        )
                )
                .add_option(
                    CreateCommandOption::new(CommandOptionType::SubCommand, "remove", "Unban a user")
                        .add_sub_option(
                            CreateCommandOption::new(CommandOptionType::User, "user", "User to unban")
                                .required(true)
                        )
                )
                .add_option(
                    CreateCommandOption::new(CommandOptionType::SubCommand, "list", "List banned users")
                        .add_sub_option(
                            CreateCommandOption::new(CommandOptionType::Integer, "page", "Page number")
                                .min_int_value(1)
                                .required(false)
                        )
                ),
        ];

        match guild_id {
            Some(gid) => {
                match gid.set_commands(&ctx.http, commands).await {
                    Ok(cmds) => println!("✅ {} commands registered on the server (instant)", cmds.len()),
                    Err(e)   => println!("❌ Failed to register server commands: {:?}", e),
                }
            }
            None => {
                match Command::set_global_commands(&ctx.http, commands).await {
                    Ok(cmds) => println!("✅ {} commands registered globally (may take up to 1 hour)", cmds.len()),
                    Err(e)   => println!("❌ Failed to register global commands: {:?}", e),
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

    // تهيئة قاعدة البيانات
    let pool = match database::init_database(&database_url).await {
        Ok(pool) => {
            println!("✅ Connected to database successfully!");
            Arc::new(pool)
        }
        Err(e) => {
            eprintln!("❌ Failed to connect to database: {}", e);
            return;
        }
    };

    // إنشاء محرك الحماية
    let security_engine = link_security::create_engine(Arc::clone(&pool));

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