use serenity::client::Context;
use serenity::model::prelude::application_command::ApplicationCommandInteraction;
use serenity::model::interactions::InteractionResponseType;
use serenity::model::interactions::InteractionApplicationCommandCallbackDataFlags;
use chrono::Utc;

// دالة مساعدة لتقليل التكرار
async fn send_response(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
    content: &str,
) {
    let _ = command
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message| {
                    message
                        .content(content)
                        .flags(InteractionApplicationCommandCallbackDataFlags::EPHEMERAL)
                })
        })
        .await;
}

pub async fn handle_clear(ctx: &Context, command: &ApplicationCommandInteraction) {
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
    let _ = command
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::DeferredChannelMessageWithSource)
                .interaction_response_data(|message| {
                    message.flags(InteractionApplicationCommandCallbackDataFlags::EPHEMERAL)
                })
        })
        .await;

    // الحصول على عدد الرسائل
    let amount = command
        .data
        .options
        .first()
        .and_then(|opt| opt.value.as_ref())
        .and_then(|val| val.as_i64())
        .map(|v| v.min(100).max(1) as usize)
        .unwrap_or(5);

    // جلب الرسائل
    let messages = match command
        .channel_id
        .messages(&ctx.http, |retriever| retriever.limit(amount as u64))
        .await
    {
        Ok(m) if m.is_empty() => {
            let _ = command.edit_original_interaction_response(&ctx.http, |r| r.content("❌ لا توجد رسائل لحذفها")).await;
            return;
        }
        Ok(m) => m,
        Err(why) => {
            let _ = command.edit_original_interaction_response(&ctx.http, |r| r.content(format!("❌ خطأ في جلب الرسائل: {}", why))).await;
            return;
        }
    };

    if is_dm {
        // في الخاص: احذف رسالة رسالة (bulk delete غير مدعوم في DMs)
        // البوت يقدر يحذف رسائله فقط في الخاص
        let bot_id = ctx.http.get_current_user().await.map(|u| u.id).unwrap_or_default();
        let deletable: Vec<_> = messages.into_iter().filter(|m| m.author.id == bot_id).collect();

        if deletable.is_empty() {
            let _ = command.edit_original_interaction_response(&ctx.http, |r| r.content("❌ لا توجد رسائل للبوت لحذفها في الخاص!")).await;
            return;
        }

        let mut deleted = 0usize;
        for msg in &deletable {
            if command.channel_id.delete_message(&ctx.http, msg.id).await.is_ok() {
                deleted += 1;
            }
        }

        let _ = command.edit_original_interaction_response(&ctx.http, |r| {
            r.content(format!("✅ تم حذف `{}` رسالة للبوت في الخاص!", deleted))
        }).await;
    } else {
        // في السيرفر: تصفية الرسائل القديمة (أكثر من 14 يوم - قيد Discord API)
        let now = Utc::now();
        let deletable: Vec<_> = messages
            .into_iter()
            .filter(|msg| {
                let msg_time: chrono::DateTime<Utc> = msg.timestamp.into();
                now.signed_duration_since(msg_time).num_days() < 14
            })
            .collect();

        if deletable.is_empty() {
            let _ = command.edit_original_interaction_response(&ctx.http, |r| r.content("❌ جميع الرسائل قديمة جداً (أكثر من 14 يوم)")).await;
            return;
        }

        match command.channel_id.delete_messages(&ctx.http, &deletable).await {
            Ok(_) => {
                let _ = command.edit_original_interaction_response(&ctx.http, |r| r.content(format!("✅ تم حذف `{}` رسالة بنجاح!", deletable.len()))).await;
            }
            Err(why) => {
                let _ = command.edit_original_interaction_response(&ctx.http, |r| r.content(format!("❌ خطأ في الحذف: {}", why))).await;
            }
        }
    }
}