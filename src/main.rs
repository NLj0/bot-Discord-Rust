use serenity::async_trait;
use serenity::client::Client;
use serenity::model::gateway::Ready;
use serenity::model::interactions::Interaction;
use serenity::model::interactions::InteractionResponseType;
use serenity::model::prelude::Message;
use serenity::prelude::*;
use serenity::client::bridge::gateway::GatewayIntents;
use std::env;
use std::sync::Arc;

mod commands;
mod database;
mod link_security;

use commands::{handle_ping, handle_clear, handle_whitelist};
use link_security::{LinkSecurityEngine, handle_message};
use serenity::prelude::TypeMapKey;

struct Handler {
    security_engine: LinkSecurityEngine,
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
        if let Interaction::ApplicationCommand(command) = interaction {
            let user_id = command.user.id.0;
            
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
                let _ = command
                    .create_interaction_response(&ctx.http, |response| {
                        response
                            .kind(InteractionResponseType::ChannelMessageWithSource)
                            .interaction_response_data(|message| {
                                message.content("❌ أنت محظور من استخدام هذا البوت")
                            })
                    })
                    .await;
                return;
            }

            // تنفيذ الأمر
            match command.data.name.as_str() {
                "ping" => {
                    handle_ping(&ctx, &command).await;
                }
                "clear" => {
                    handle_clear(&ctx, &command).await;
                }
                "whitelist" => {
                    handle_whitelist(&ctx, &command).await;
                }
                _ => {
                    let _ = command
                        .create_interaction_response(&ctx.http, |response| {
                            response
                                .kind(InteractionResponseType::ChannelMessageWithSource)
                                .interaction_response_data(|message| message.content("Unknown command"))
                        })
                        .await;
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
        
        // تسجيل أمر /ping
        if let Err(why) = serenity::model::interactions::application_command::ApplicationCommand::create_global_application_command(&ctx.http, |command| {
            command
                .name("ping")
                .description("استجابة البيينج مع قياس السرعة")
        })
        .await
        {
            println!("Error registering ping command: {:?}", why);
        } else {
            println!("✅ Ping command registered!");
        }
        
        // تسجيل أمر /clear
        if let Err(why) = serenity::model::interactions::application_command::ApplicationCommand::create_global_application_command(&ctx.http, |command| {
            command
                .name("clear")
                .description("حذف الرسائل من القناة")
                .create_option(|option| {
                    option
                        .name("amount")
                        .description("عدد الرسائل المراد حذفها (1-100)")
                        .kind(serenity::model::interactions::application_command::ApplicationCommandOptionType::Integer)
                        .min_int_value(1)
                        .max_int_value(100)
                        .required(false)
                })
        })
        .await
        {
            println!("Error registering clear command: {:?}", why);
        } else {
            println!("✅ Clear command registered!");
        }

        // تسجيل أمر /whitelist
        if let Err(why) = serenity::model::interactions::application_command::ApplicationCommand::create_global_application_command(&ctx.http, |command| {
            command
                .name("whitelist")
                .description("إدارة القائمة البيضاء للروابط")
                .create_option(|opt| {
                    opt.name("add")
                        .description("إضافة رابط أو domain للقائمة البيضاء")
                        .kind(serenity::model::interactions::application_command::ApplicationCommandOptionType::SubCommand)
                        .create_sub_option(|s| {
                            s.name("domain")
                                .description("الرابط أو الـ domain مثال: youtube.com")
                                .kind(serenity::model::interactions::application_command::ApplicationCommandOptionType::String)
                                .required(true)
                        })
                })
                .create_option(|opt| {
                    opt.name("remove")
                        .description("حذف domain من القائمة البيضاء")
                        .kind(serenity::model::interactions::application_command::ApplicationCommandOptionType::SubCommand)
                        .create_sub_option(|s| {
                            s.name("domain")
                                .description("الـ domain المراد حذفه")
                                .kind(serenity::model::interactions::application_command::ApplicationCommandOptionType::String)
                                .required(true)
                        })
                })
        })
        .await
        {
            println!("Error registering whitelist command: {:?}", why);
        } else {
            println!("✅ Whitelist command registered!");
        }

    }
}

#[tokio::main]
async fn main() {
    // Load environment variables
    dotenv::dotenv().ok();
    
    let token = env::var("DISCORD_TOKEN")
        .expect("Expected a token in the environment");

    let application_id: u64 = env::var("APPLICATION_ID")
        .expect("Expected APPLICATION_ID in the environment")
        .parse()
        .expect("APPLICATION_ID should be a valid u64");

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

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::GUILDS
        | GatewayIntents::from_bits_truncate(1 << 15); // MESSAGE_CONTENT (privileged)

    let mut client = Client::builder(&token)
        .event_handler(Handler {
            security_engine,
        })
        .application_id(application_id)
        .intents(intents)
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
}