use serenity::async_trait;
use serenity::client::Client;
use serenity::model::gateway::Ready;
use serenity::model::interactions::Interaction;
use serenity::model::interactions::InteractionResponseType;
use serenity::prelude::*;
use serenity::client::bridge::gateway::GatewayIntents;
use std::env;
use std::sync::Arc;

mod commands;
mod database;
use commands::{handle_ping, handle_clear};
use database::UserStats;
use serenity::prelude::TypeMapKey;

struct Handler;

// كمفتاح لتخزين قاعدة البيانات في TypeMap
pub struct DatabaseKey;

impl TypeMapKey for DatabaseKey {
    type Value = Arc<sqlx::mysql::MySqlPool>;
}

#[async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::ApplicationCommand(command) = interaction {
            match command.data.name.as_str() {
                "ping" => {
                    handle_ping(&ctx, &command).await;
                }
                "clear" => {
                    handle_clear(&ctx, &command).await;
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
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
        
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
    }
}

#[tokio::main]
async fn main() {
    // Load environment variables from .env file
    dotenv::dotenv().ok();
    
    let token = env::var("DISCORD_TOKEN")
        .expect("Expected a token in the environment");

    let application_id: u64 = env::var("APPLICATION_ID")
        .expect("Expected APPLICATION_ID in the environment")
        .parse()
        .expect("APPLICATION_ID should be a valid u64");

    let database_url = env::var("DATABASE_URL")
        .expect("Expected DATABASE_URL in the environment");

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

    let intents = GatewayIntents::GUILD_MESSAGES 
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::GUILDS;

    let mut client = Client::builder(&token)
        .event_handler(Handler)
        .application_id(application_id)
        .intents(intents)
        .await
        .expect("Error creating client");

    // إضافة قاعدة البيانات إلى بيانات العميل
    {
        let mut data = client.data.write().await;
        data.insert::<DatabaseKey>(pool);
    }

    if let Err(why) = client.start().await {
        eprintln!("Client error: {:?}", why);
    }
}
