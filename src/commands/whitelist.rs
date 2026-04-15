use serenity::client::Context;
use serenity::model::application::{CommandInteraction, CommandDataOptionValue};
use serenity::builder::{CreateInteractionResponse, CreateInteractionResponseMessage};
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

pub async fn handle_whitelist(ctx: &Context, command: &CommandInteraction) {
    // التحقق من الصلاحيات (يجب أن يكون Admin)
    let has_perm = command
        .member
        .as_ref()
        .and_then(|m| m.permissions)
        .map(|p| p.administrator())
        .unwrap_or(false);

    if !has_perm {
        let _ = command.create_response(&ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content("❌ هذا الأمر للـ Admins فقط")
                    .ephemeral(true)
            )
        ).await;
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

    let input = sub_opts
        .first()
        .and_then(|o| {
            if let CommandDataOptionValue::String(s) = &o.value {
                Some(s.clone())
            } else {
                None
            }
        })
        .unwrap_or_default();

    if input.is_empty() {
        let _ = command.create_response(&ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content("❌ الرجاء إدخال رابط أو domain")
                    .ephemeral(true)
            )
        ).await;
        return;
    }

    let domain = extract_domain(&input);

    let pool = match ctx.data.read().await.get::<DatabaseKey>().cloned() {
        Some(p) => p,
        None => {
            let _ = command.create_response(&ctx.http,
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new().content("❌ Database error")
                )
            ).await;
            return;
        }
    };

    let user_id = command.user.id.get();
    let guild_id = command.guild_id.map(|g| g.get()).unwrap_or(0);
    let msg = match sub_name {
        "add" => match add_to_whitelist(&*pool, &domain, user_id, guild_id).await {
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

    let _ = command.create_response(&ctx.http,
        CreateInteractionResponse::Message(
            CreateInteractionResponseMessage::new()
                .content(msg)
                .ephemeral(true)
        )
    ).await;
}
