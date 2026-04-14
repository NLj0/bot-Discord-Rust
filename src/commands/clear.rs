use serenity::client::Context;
use serenity::model::application::{CommandInteraction, CommandDataOptionValue};
use serenity::builder::{CreateInteractionResponse, CreateInteractionResponseMessage, EditInteractionResponse, GetMessages};
use chrono::Utc;

// دالة مساعدة لتقليل التكرار
async fn send_response(
    ctx: &Context,
    command: &CommandInteraction,
    content: &str,
) {
    let _ = command.create_response(&ctx.http,
        CreateInteractionResponse::Message(
            CreateInteractionResponseMessage::new()
                .content(content)
                .ephemeral(true)
        )
    ).await;
}

pub async fn handle_clear(ctx: &Context, command: &CommandInteraction) {
    let is_dm = command.member.is_none();

    // في السيرفر: تحقق من صلاحية manage_messages
    if !is_dm {
        let perms = command
            .member
            .as_ref()
            .and_then(|m| m.permissions)
            .unwrap_or_default();
        if !perms.manage_messages() {
            send_response(ctx, command, "❌ ليس لديك صلاحية حذف الرسائل!").await;
            return;
        }
    }

    // إرسال deferred response مباشرة لتجنب timeout الـ 3 ثواني
    let _ = command.create_response(&ctx.http,
        CreateInteractionResponse::Defer(
            CreateInteractionResponseMessage::new().ephemeral(true)
        )
    ).await;

    // الحصول على عدد الرسائل
    let amount = command
        .data
        .options
        .first()
        .and_then(|opt| {
            if let CommandDataOptionValue::Integer(v) = opt.value {
                Some(v.min(100).max(1) as u8)
            } else {
                None
            }
        })
        .unwrap_or(5);

    // جلب الرسائل
    let messages = match command
        .channel_id
        .messages(&ctx.http, GetMessages::new().limit(amount))
        .await
    {
        Ok(m) if m.is_empty() => {
            let _ = command.edit_response(&ctx.http, EditInteractionResponse::new().content("❌ لا توجد رسائل لحذفها")).await;
            return;
        }
        Ok(m) => m,
        Err(why) => {
            let _ = command.edit_response(&ctx.http, EditInteractionResponse::new().content(format!("❌ خطأ في جلب الرسائل: {}", why))).await;
            return;
        }
    };

    if is_dm {
        // في الخاص: احذف رسالة رسالة (bulk delete غير مدعوم في DMs)
        // البوت يقدر يحذف رسائله فقط في الخاص
        let bot_id = match ctx.http.get_current_user().await {
            Ok(user) => user.id,
            Err(_) => {
                let _ = command.edit_response(&ctx.http, EditInteractionResponse::new().content("❌ خطأ في الحصول على معلومات البوت")).await;
                return;
            }
        };
        let deletable: Vec<_> = messages.into_iter().filter(|m| m.author.id == bot_id).collect();

        if deletable.is_empty() {
            let _ = command.edit_response(&ctx.http, EditInteractionResponse::new().content("❌ لا توجد رسائل للبوت لحذفها في الخاص!")).await;
            return;
        }

        let mut deleted = 0usize;
        for msg in &deletable {
            if command.channel_id.delete_message(&ctx.http, msg.id).await.is_ok() {
                deleted += 1;
            }
        }

        let _ = command.edit_response(&ctx.http,
            EditInteractionResponse::new().content(format!("✅ تم حذف `{}` رسالة للبوت في الخاص!", deleted))
        ).await;
    } else {
        // في السيرفر: تصفية الرسائل القديمة (أكثر من 14 يوم - قيد Discord API)
        let now_ts = Utc::now().timestamp();
        let deletable: Vec<_> = messages
            .into_iter()
            .filter(|msg| now_ts - msg.timestamp.unix_timestamp() < 14 * 24 * 3600)
            .collect();

        if deletable.is_empty() {
            let _ = command.edit_response(&ctx.http, EditInteractionResponse::new().content("❌ جميع الرسائل قديمة جداً (أكثر من 14 يوم)")).await;
            return;
        }

        match command.channel_id.delete_messages(&ctx.http, &deletable).await {
            Ok(_) => {
                let _ = command.edit_response(&ctx.http, EditInteractionResponse::new().content(format!("✅ تم حذف `{}` رسالة بنجاح!", deletable.len()))).await;
            }
            Err(why) => {
                let _ = command.edit_response(&ctx.http, EditInteractionResponse::new().content(format!("❌ خطأ في الحذف: {}", why))).await;
            }
        }
    }
}