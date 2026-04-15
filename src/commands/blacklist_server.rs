use serenity::client::Context;
use serenity::model::application::{CommandInteraction, CommandDataOptionValue};
use serenity::builder::{CreateInteractionResponse, CreateInteractionResponseMessage};
use crate::database::{
    add_server_to_blacklist, remove_server_from_blacklist, get_blacklisted_servers_paginated,
};
use crate::DatabaseKey;

const PER_PAGE: i64 = 5;

fn is_admin(command: &CommandInteraction) -> bool {
    command
        .member
        .as_ref()
        .and_then(|m| m.permissions)
        .map(|p| p.administrator())
        .unwrap_or(false)
}

async fn reply(ctx: &Context, command: &CommandInteraction, content: impl Into<String>, ephemeral: bool) {
    let _ = command
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(content.into())
                    .ephemeral(ephemeral),
            ),
        )
        .await;
}

pub async fn handle_blacklist_server(ctx: &Context, command: &CommandInteraction) {
    if !is_admin(command) {
        reply(ctx, command, "❌ This command is for Admins only.", true).await;
        return;
    }

    let subcommand = match command.data.options.first() {
        Some(s) => s,
        None => return,
    };

    let (sub_name, sub_opts) = match &subcommand.value {
        CommandDataOptionValue::SubCommand(opts) => (subcommand.name.as_str(), opts),
        _ => return,
    };

    let pool = match ctx.data.read().await.get::<DatabaseKey>().cloned() {
        Some(p) => p,
        None => {
            reply(ctx, command, "❌ Database error.", true).await;
            return;
        }
    };

    match sub_name {
        "add" => {
            let guild_id_str = sub_opts
                .iter()
                .find(|o| o.name == "guild_id")
                .and_then(|o| {
                    if let CommandDataOptionValue::String(s) = &o.value {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or_default();

            let reason = sub_opts
                .iter()
                .find(|o| o.name == "reason")
                .and_then(|o| {
                    if let CommandDataOptionValue::String(s) = &o.value {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| "No reason provided".to_string());

            let guild_id: u64 = match guild_id_str.parse() {
                Ok(id) => id,
                Err(_) => {
                    reply(ctx, command, "❌ Invalid guild ID — must be a numeric snowflake.", true).await;
                    return;
                }
            };

            let blocked_by = command.user.id.get();

            match add_server_to_blacklist(&*pool, guild_id, "", &reason, blocked_by).await {
                Ok(true) => {
                    reply(
                        ctx,
                        command,
                        format!(
                            "🚫 Server `{}` has been blacklisted.\n**Reason:** {}",
                            guild_id, reason
                        ),
                        false,
                    )
                    .await
                }
                Ok(false) => {
                    reply(
                        ctx,
                        command,
                        format!("ℹ️ Server `{}` is already blacklisted.", guild_id),
                        true,
                    )
                    .await
                }
                Err(e) => reply(ctx, command, format!("❌ Error: {}", e), true).await,
            }
        }

        "remove" => {
            let guild_id_str = sub_opts
                .iter()
                .find(|o| o.name == "guild_id")
                .and_then(|o| {
                    if let CommandDataOptionValue::String(s) = &o.value {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or_default();

            let guild_id: u64 = match guild_id_str.parse() {
                Ok(id) => id,
                Err(_) => {
                    reply(ctx, command, "❌ Invalid guild ID — must be a numeric snowflake.", true).await;
                    return;
                }
            };

            match remove_server_from_blacklist(&*pool, guild_id).await {
                Ok(_) => {
                    reply(
                        ctx,
                        command,
                        format!("✅ Server `{}` removed from blacklist.", guild_id),
                        false,
                    )
                    .await
                }
                Err(e) => reply(ctx, command, format!("❌ Error: {}", e), true).await,
            }
        }

        "list" => {
            let page = sub_opts
                .iter()
                .find(|o| o.name == "page")
                .and_then(|o| {
                    if let CommandDataOptionValue::Integer(n) = o.value {
                        Some(n.max(1))
                    } else {
                        None
                    }
                })
                .unwrap_or(1) as i64;

            match get_blacklisted_servers_paginated(&*pool, page, PER_PAGE).await {
                Ok((servers, total)) => {
                    if servers.is_empty() {
                        reply(ctx, command, "✅ No blacklisted servers.", true).await;
                        return;
                    }
                    let total_pages = (total + PER_PAGE - 1) / PER_PAGE;
                    let mut msg =
                        format!("🚫 **Blacklisted Servers** — Page {}/{}\n\n", page, total_pages);
                    for s in &servers {
                        msg.push_str(&format!(
                            "🔹 `{}` {}\n   Reason: {}\n   Blocked by: <@{}> | {}\n\n",
                            s.server_id,
                            if s.server_name.is_empty() {
                                String::new()
                            } else {
                                format!("| **{}**", s.server_name)
                            },
                            s.reason,
                            s.blocked_by,
                            s.created_at,
                        ));
                    }
                    reply(ctx, command, msg, true).await;
                }
                Err(e) => reply(ctx, command, format!("❌ Error: {}", e), true).await,
            }
        }

        _ => reply(ctx, command, "❌ Unknown subcommand.", true).await,
    }
}
