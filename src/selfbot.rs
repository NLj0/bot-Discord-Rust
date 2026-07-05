use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use std::env;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};

const GATEWAY_URL: &str = "wss://gateway.discord.gg/?v=10&encoding=json";

#[derive(Debug, Deserialize)]
struct GatewayUser {
    id: String,
    username: String,
    bot: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct MessageReference {
    message_id: Option<String>,
    channel_id: Option<String>,
    guild_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Attachment {
    filename: String,
    content_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GatewayMessage {
    content: String,
    channel_id: String,
    guild_id: Option<String>,
    timestamp: String,
    author: GatewayUser,
    referenced_message: Option<Box<GatewayMessage>>,
    message_reference: Option<MessageReference>,
    attachments: Option<Vec<Attachment>>,
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();

    println!("\n🔧 ============================================================");
    println!("⚙️  تهيئة Selfbot (User Gateway)...");
    println!("============================================================");

    let token = env::var("USER_TOKEN").expect("❌ USER_TOKEN غير موجود في .env");
    let channel_filter = env::var("CHANNEL_ID").ok();
    let server_filter = env::var("SERVER_ID")
        .or_else(|_| env::var("GUILD_ID"))
        .ok();

    if let Some(ref channel_id) = channel_filter {
        println!("🎯 فلتر القناة: {}", channel_id);
    }
    if let Some(ref server_id) = server_filter {
        println!("🎯 فلتر السيرفر: {}", server_id);
    }

    println!("🔄 جاري الاتصال بـ Discord...\n");

    loop {
        match run_gateway(&token, channel_filter.as_deref(), server_filter.as_deref()).await {
            Ok(()) => {
                println!("🔌 انقطع الاتصال، إعادة المحاولة بعد 5 ثوان...");
            }
            Err(error) => {
                eprintln!("❌ خطأ: {}", error);
                eprintln!("🔄 إعادة المحاولة بعد 5 ثوان...");
            }
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn run_gateway(
    token: &str,
    channel_filter: Option<&str>,
    server_filter: Option<&str>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (ws_stream, _) = connect_async(GATEWAY_URL).await?;
    let (write, mut read) = ws_stream.split();
    let write = Arc::new(Mutex::new(write));
    let last_sequence = Arc::new(Mutex::new(None::<u64>));
    let mut heartbeat_task: Option<tokio::task::JoinHandle<()>> = None;

    while let Some(message) = read.next().await {
        let message = message?;
        let text = match message {
            WsMessage::Text(text) => text,
            WsMessage::Close(frame) => {
                return Err(format!("Gateway closed: {:?}", frame).into());
            }
            WsMessage::Ping(data) => {
                write.lock().await.send(WsMessage::Pong(data)).await?;
                continue;
            }
            _ => continue,
        };

        let payload: Value = serde_json::from_str(&text)?;
        let opcode = payload["op"].as_u64().unwrap_or(999);

        if let Some(sequence) = payload["s"].as_u64() {
            *last_sequence.lock().await = Some(sequence);
        }

        match opcode {
            10 => {
                let interval_ms = payload["d"]["heartbeat_interval"]
                    .as_u64()
                    .ok_or("Missing heartbeat interval")?;

                if let Some(task) = heartbeat_task.take() {
                    task.abort();
                }

                let heartbeat_write = Arc::clone(&write);
                let heartbeat_sequence = Arc::clone(&last_sequence);

                heartbeat_task = Some(tokio::spawn(async move {
                    let mut ticker = interval(Duration::from_millis(interval_ms));
                    loop {
                        ticker.tick().await;
                        let sequence = *heartbeat_sequence.lock().await;
                        let heartbeat = json!({ "op": 1, "d": sequence });
                        if heartbeat_write
                            .lock()
                            .await
                            .send(WsMessage::Text(heartbeat.to_string()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                }));

                let identify = build_identify_payload(token);
                write
                    .lock()
                    .await
                    .send(WsMessage::Text(identify.to_string()))
                    .await?;
            }
            0 => match payload["t"].as_str() {
                Some("READY") => {
                    let username = payload["d"]["user"]["username"]
                        .as_str()
                        .unwrap_or("unknown");
                    let user_id = payload["d"]["user"]["id"]
                        .as_str()
                        .unwrap_or("unknown");

                    println!("🚀 ============================================================");
                    println!("✅ Selfbot متصل بنجاح!");
                    println!("👤 الحساب: {}", username);
                    println!("🆔 ID: {}", user_id);
                    println!("📡 جاهز لقراءة الرسائل...");
                    println!("============================================================\n");
                }
                Some("MESSAGE_CREATE") => {
                    if let Ok(message) =
                        serde_json::from_value::<GatewayMessage>(payload["d"].clone())
                    {
                        handle_message(&message, channel_filter, server_filter);
                    }
                }
                _ => {}
            },
            7 | 9 => {
                return Err(format!("Discord gateway error (op {}): {}", opcode, text).into());
            }
            _ => {}
        }

        if let Some(task) = &heartbeat_task {
            if task.is_finished() {
                return Err("Heartbeat task stopped".into());
            }
        }
    }

    Ok(())
}

fn build_identify_payload(token: &str) -> Value {
    json!({
        "op": 2,
        "d": {
            "token": token,
            "capabilities": 8189,
            "properties": {
                "os": "Windows",
                "browser": "Chrome",
                "device": "",
                "system_locale": "ar-SA",
                "browser_user_agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
                "browser_version": "120.0.0.0",
                "os_version": "10",
                "referrer": "",
                "referring_domain": "",
                "referrer_current": "",
                "referring_domain_current": "",
                "release_channel": "stable",
                "client_build_number": 294375,
                "client_event_source": null
            },
            "presence": {
                "status": "online",
                "since": 0,
                "activities": [],
                "afk": false
            },
            "compress": false,
            "client_state": {
                "guild_versions": {}
            }
        }
    })
}

fn handle_message(
    message: &GatewayMessage,
    channel_filter: Option<&str>,
    server_filter: Option<&str>,
) {
    if message.author.bot.unwrap_or(false) {
        return;
    }

    if let Some(filter) = channel_filter {
        if message.channel_id != filter {
            return;
        }
    }

    if let Some(filter) = server_filter {
        if message.guild_id.as_deref() != Some(filter) {
            return;
        }
    }

    let content = clean_text(&message.content);
    if content.is_empty() {
        return;
    }

    println!("\n============================================================");
    println!("📨 رسالة جديدة");
    println!("============================================================");
    println!(
        "👤 المرسل: {} (ID: {})",
        message.author.username, message.author.id
    );
    println!("📍 القناة ID: {}", message.channel_id);

    if let Some(guild_id) = &message.guild_id {
        println!("🏠 السيرفر ID: {}", guild_id);
    }

    if let Some(referenced) = &message.referenced_message {
        println!(
            "↩️  رد على: {} (ID: {})",
            referenced.author.username, referenced.author.id
        );
        let ref_content = clean_text(&referenced.content);
        if !ref_content.is_empty() {
            println!("   💬 الرسالة الأصلية: \"{}\"", truncate(&ref_content, 100));
        }
    } else if let Some(reference) = &message.message_reference {
        if let Some(message_id) = &reference.message_id {
            println!("↩️  رد على رسالة ID: {}", message_id);
        }
        if let Some(reference_channel) = &reference.channel_id {
            println!("   📍 في قناة ID: {}", reference_channel);
        }
    }

    println!("🕐 الوقت: {}", message.timestamp);
    println!("💬 المحتوى:");
    println!("   \"{}\"", content);

    if let Some(attachments) = &message.attachments {
        if !attachments.is_empty() {
            println!("📎 المرفقات ({}):", attachments.len());
            for (index, attachment) in attachments.iter().enumerate() {
                let content_type = attachment
                    .content_type
                    .as_deref()
                    .unwrap_or("unknown");
                println!(
                    "   {}. {} ({})",
                    index + 1,
                    attachment.filename,
                    content_type
                );
            }
        }
    }

    println!("============================================================\n");
}

fn clean_text(text: &str) -> String {
    text.chars()
        .filter(|character| {
            matches!(
                character,
                '\u{0600}'..='\u{06FF}'
                    | 'a'..='z'
                    | 'A'..='Z'
                    | '0'..='9'
                    | ' '
                    | '.'
                    | ','
                    | '!'
                    | '?'
                    | ':'
                    | ';'
                    | '-'
                    | '_'
                    | '\n'
            )
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        format!("{}...", text.chars().take(max_chars).collect::<String>())
    }
}
