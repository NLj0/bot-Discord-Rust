use serenity::client::Context;
use serenity::model::application::{CommandInteraction, CommandDataOptionValue};
use serenity::builder::{CreateInteractionResponse, CreateInteractionResponseMessage};
use crate::database::{add_warning, remove_last_warning, get_warnings_paginated, ensure_user};
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

pub async fn handle_warn(ctx: &Context, command: &CommandInteraction) {
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
            let user_id = sub_opts
                .iter()
                .find(|o| o.name == "user")
                .and_then(|o| {
                    if let CommandDataOptionValue::User(u) = o.value {
                        Some(u.get())
                    } else {
                        None
                    }
                });

            let user_id = match user_id {
                Some(id) => id,
                None => {
                    reply(ctx, command, "❌ Please specify a user.", true).await;
                    return;
                }
            };

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

            let moderator_id = command.user.id.get();
            let guild_id = command.guild_id.map(|g| g.get()).unwrap_or(0);

            let _ = ensure_user(&*pool, user_id, &user_id.to_string()).await;

            match add_warning(&*pool, user_id, &reason, moderator_id, guild_id).await {
                Ok(count) => {
                    reply(
                        ctx,
                        command,
                        format!(
                            "⚠️ <@{}> has been warned.\n**Reason:** {}\n**Total warnings:** {}",
                            user_id, reason, count
                        ),
                        false,
                    )
                    .await
                }
                Err(e) => reply(ctx, command, format!("❌ Error: {}", e), true).await,
            }
        }

        "remove" => {
            let user_id = sub_opts
                .iter()
                .find(|o| o.name == "user")
                .and_then(|o| {
                    if let CommandDataOptionValue::User(u) = o.value {
                        Some(u.get())
                    } else {
                        None
                    }
                });

            let user_id = match user_id {
                Some(id) => id,
                None => {
                    reply(ctx, command, "❌ Please specify a user.", true).await;
                    return;
                }
            };

            match remove_last_warning(&*pool, user_id).await {
                Ok(true) => {
                    reply(
                        ctx,
                        command,
                        format!("✅ Latest warning removed from <@{}>.", user_id),
                        false,
                    )
                    .await
                }
                Ok(false) => {
                    reply(
                        ctx,
                        command,
                        format!("ℹ️ <@{}> has no active warnings.", user_id),
                        true,
                    )
                    .await
                }
                Err(e) => reply(ctx, command, format!("❌ Error: {}", e), true).await,
            }
        }

        "list" => {
            let user_id = sub_opts.iter().find(|o| o.name == "user").and_then(|o| {
                if let CommandDataOptionValue::User(u) = o.value {
                    Some(u.get())
                } else {
                    None
                }
            });

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

            match get_warnings_paginated(&*pool, user_id, page, PER_PAGE).await {
                Ok((warnings, total)) => {
                    if warnings.is_empty() {
                        reply(ctx, command, "✅ No warnings found.", true).await;
                        return;
                    }
                    let total_pages = (total + PER_PAGE - 1) / PER_PAGE;
                    let header = match user_id {
                        Some(uid) => format!(
                            "⚠️ **Warnings for <@{}>** — Page {}/{}\n\n",
                            uid, page, total_pages
                        ),
                        None => format!("⚠️ **All Warnings** — Page {}/{}\n\n", page, total_pages),
                    };
                    let mut msg = header;
                    for w in &warnings {
                        msg.push_str(&format!(
                            "🔹 `#{}` <@{}>\n   Reason: {}\n   By: <@{}> | {}\n\n",
                            w.id, w.user_id, w.reason, w.moderator_id, w.created_at
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
