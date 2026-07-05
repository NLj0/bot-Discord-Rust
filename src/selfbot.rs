mod selfbot_data;

use selfbot_data::{
    clean_text, truncate, IncomingMessage, IncomingReply, MessageStore, ProcessOutcome,
};
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
}

#[derive(Debug, Deserialize)]
struct Attachment {
    filename: String,
    content_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GatewayMessage {
    id: String,
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

    println!("\n============================================================");
    println!("Selfbot + Data Collector");
    println!("============================================================");

    let token = env::var("USER_TOKEN").expect("USER_TOKEN missing in .env");
    let channel_filter = env::var("CHANNEL_ID").ok();
    let server_filter = env::var("SERVER_ID")
        .or_else(|_| env::var("GUILD_ID"))
        .ok();
    let data_dir = env::var("DATA_DIR").unwrap_or_else(|_| "data".to_string());
    let backfill = env::var("BACKFILL")
        .ok()
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(true);

    let store = Arc::new(Mutex::new(
        MessageStore::load(&data_dir)
            .await
            .expect("Failed to initialize data store"),
    ));

    if backfill {
        if let Err(error) = backfill_recent_messages(
            &token,
            channel_filter.as_deref(),
            server_filter.as_deref(),
            Arc::clone(&store),
        )
        .await
        {
            eprintln!("Backfill warning: {}", error);
        }
    }

    if let Some(channel_id) = &channel_filter {
        println!("Channel filter: {}", channel_id);
    }
    if let Some(server_id) = &server_filter {
        println!("Server filter: {}", server_id);
    }
    println!("Data directory: {}", data_dir);
    println!("Connecting to Discord...\n");

    loop {
        let store = Arc::clone(&store);
        match run_gateway(
            &token,
            channel_filter.as_deref(),
            server_filter.as_deref(),
            store,
        )
        .await
        {
            Ok(()) => println!("Disconnected. Retrying in 5 seconds..."),
            Err(error) => eprintln!("Error: {}. Retrying in 5 seconds...", error),
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn run_gateway(
    token: &str,
    channel_filter: Option<&str>,
    server_filter: Option<&str>,
    store: Arc<Mutex<MessageStore>>,
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
            WsMessage::Close(frame) => return Err(format!("Gateway closed: {:?}", frame).into()),
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

                write
                    .lock()
                    .await
                    .send(WsMessage::Text(
                        build_identify_payload(token).to_string(),
                    ))
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

                    println!("Connected as {} ({})", username, user_id);
                    println!("Listening for messages...\n");
                }
                Some("MESSAGE_CREATE") => {
                    if let Ok(message) =
                        serde_json::from_value::<GatewayMessage>(payload["d"].clone())
                    {
                        handle_message(&message, channel_filter, server_filter, &store).await;
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

async fn handle_message(
    message: &GatewayMessage,
    channel_filter: Option<&str>,
    server_filter: Option<&str>,
    store: &Arc<Mutex<MessageStore>>,
) {
    if let Some(filter) = channel_filter {
        if message.channel_id != filter {
            return;
        }
    }

    if let Some(filter) = server_filter {
        if let Some(guild_id) = message.guild_id.as_deref() {
            if guild_id != filter {
                return;
            }
        }
    }

    let reply_to = message.referenced_message.as_ref().map(|referenced| {
        IncomingReply {
            message_id: referenced.id.clone(),
            author_id: referenced.author.id.clone(),
            author_name: referenced.author.username.clone(),
            content: referenced.content.clone(),
        }
    });

    let incoming = IncomingMessage {
        id: message.id.clone(),
        author_id: message.author.id.clone(),
        author_name: message.author.username.clone(),
        content: message.content.clone(),
        channel_id: message.channel_id.clone(),
        guild_id: message.guild_id.clone(),
        timestamp: message.timestamp.clone(),
        is_bot: message.author.bot.unwrap_or(false),
        reply_to,
    };

    let mut store = store.lock().await;
    let outcome = match store.process(incoming).await {
        Ok(outcome) => outcome,
        Err(error) => {
            eprintln!("Failed to process message {}: {}", message.id, error);
            return;
        }
    };

    let cleaned = clean_text(&message.content);
    if cleaned.is_empty() {
        return;
    }

    match outcome {
        ProcessOutcome::Saved => {
            println!("\n============================================================");
            println!("Saved message");
            println!("============================================================");
            println!("Author: {} ({})", message.author.username, message.author.id);
            println!("Channel: {}", message.channel_id);
            if let Some(guild_id) = &message.guild_id {
                println!("Server: {}", guild_id);
            }
            if let Some(referenced) = &message.referenced_message {
                println!(
                    "Reply to: {} ({})",
                    referenced.author.username, referenced.author.id
                );
                let ref_content = clean_text(&referenced.content);
                if !ref_content.is_empty() {
                    println!("Original: \"{}\"", truncate(&ref_content, 100));
                }
            } else if let Some(reference) = &message.message_reference {
                if let Some(message_id) = &reference.message_id {
                    println!("Reply to message ID: {}", message_id);
                }
            }
            println!("Time: {}", message.timestamp);
            println!("Content: \"{}\"", cleaned);
            print_stats(&store);
            println!("============================================================\n");
        }
        ProcessOutcome::SkippedDuplicateId => {
            println!("Skipped duplicate message ID: {}", message.id);
        }
        ProcessOutcome::SkippedDuplicateContent => {
            println!("Skipped duplicate content from {}", message.author.username);
        }
        ProcessOutcome::SkippedLowQuality => {
            println!("Skipped low quality message from {}", message.author.username);
        }
        ProcessOutcome::SkippedBot | ProcessOutcome::SkippedEmpty => {}
    }
}

async fn backfill_recent_messages(
    token: &str,
    channel_filter: Option<&str>,
    server_filter: Option<&str>,
    store: Arc<Mutex<MessageStore>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let Some(channel_id) = channel_filter else {
        return Ok(());
    };

    let client = reqwest::Client::new();
    let response = client
        .get(format!(
            "https://discord.com/api/v10/channels/{channel_id}/messages?limit=50"
        ))
        .header("Authorization", token)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(format!("Backfill failed with status {}", response.status()).into());
    }

    let messages: Vec<GatewayMessage> = response.json().await?;
    println!("Backfilling {} recent messages...", messages.len());

    for message in messages.into_iter().rev() {
        handle_message(&message, Some(channel_id), server_filter, &store).await;
    }

    let stats = store.lock().await.stats().clone();
    println!(
        "Backfill done. saved={} duplicates={} low_quality={}",
        stats.saved,
        stats.skipped_duplicate_id + stats.skipped_duplicate_content,
        stats.skipped_low_quality
    );

    Ok(())
}

fn print_stats(store: &MessageStore) {
    let stats = store.stats();
    println!(
        "Stats: saved={} duplicates={} low_quality={} total_seen={}",
        stats.saved,
        stats.skipped_duplicate_id + stats.skipped_duplicate_content,
        stats.skipped_low_quality,
        stats.total_seen
    );
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
