use serenity::client::Context;
use serenity::model::prelude::application_command::ApplicationCommandInteraction;
use serenity::model::interactions::InteractionResponseType;
use serenity::model::interactions::InteractionApplicationCommandCallbackDataFlags;
use crate::database::{add_to_whitelist, remove_from_whitelist};
use crate::DatabaseKey;

fn extract_domain(url_or_domain: &str) -> String {
    url_or_domain
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or(url_or_domain)
        .split('?')
        .next()
        .unwrap_or(url_or_domain)
        .to_lowercase()
}

pub async fn handle_whitelist(ctx: &Context, command: &ApplicationCommandInteraction) {
    // التحقق من الصلاحيات (يجب أن يكون Admin)
    let has_perm = command
        .member
        .as_ref()
        .and_then(|m| m.permissions)
        .map(|p| p.administrator())
        .unwrap_or(false);

    if !has_perm {
        let _ = command
            .create_interaction_response(&ctx.http, |r| {
                r.kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|m| {
                        m.content("❌ هذا الأمر للـ Admins فقط")
                            .flags(InteractionApplicationCommandCallbackDataFlags::EPHEMERAL)
                    })
            })
            .await;
        return;
    }

    let subcommand = match command.data.options.first() {
        Some(s) => s,
        None => return,
    };

    let input = subcommand
        .options
        .first()
        .and_then(|o| o.value.as_ref())
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if input.is_empty() {
        let _ = command
            .create_interaction_response(&ctx.http, |r| {
                r.kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|m| {
                        m.content("❌ الرجاء إدخال رابط أو domain")
                            .flags(InteractionApplicationCommandCallbackDataFlags::EPHEMERAL)
                    })
            })
            .await;
        return;
    }

    let domain = extract_domain(&input);

    let pool = match ctx.data.read().await.get::<DatabaseKey>().cloned() {
        Some(p) => p,
        None => {
            let _ = command
                .create_interaction_response(&ctx.http, |r| {
                    r.kind(InteractionResponseType::ChannelMessageWithSource)
                        .interaction_response_data(|m| m.content("❌ خطأ في الداتا بيس"))
                })
                .await;
            return;
        }
    };

    let user_id = command.user.id.0;
    let msg = match subcommand.name.as_str() {
        "add" => match add_to_whitelist(&*pool, &domain, user_id).await {
            Ok(true)  => format!("✅ تم إضافة `{}` للقائمة البيضاء", domain),
            Ok(false) => format!("ℹ️ `{}` موجود مسبقاً في القائمة البيضاء", domain),
            Err(e)    => format!("❌ خطأ: {}", e),
        },
        "remove" => match remove_from_whitelist(&*pool, &domain).await {
            Ok(_)  => format!("🗑️ تم حذف `{}` من القائمة البيضاء", domain),
            Err(e) => format!("❌ خطأ: {}", e),
        },
        _ => "❌ subcommand غير معروف".to_string(),
    };

    let _ = command
        .create_interaction_response(&ctx.http, |r| {
            r.kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|m| {
                    m.content(msg)
                        .flags(InteractionApplicationCommandCallbackDataFlags::EPHEMERAL)
                })
        })
        .await;
}
