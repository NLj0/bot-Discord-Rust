use serenity::client::Context;
use serenity::model::prelude::application_command::ApplicationCommandInteraction;
use serenity::model::interactions::InteractionResponseType;
use serenity::model::interactions::InteractionApplicationCommandCallbackDataFlags;

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
    // التحقق من صلاحيات المستخدم
    if let Some(member) = &command.member {
        let perms = member.permissions.unwrap_or_default();
        if !perms.manage_messages() {
            send_response(ctx, command, "❌ ليس لديك صلاحية حذف الرسائل!").await;
            return;
        }
    }

    // الحصول على عدد الرسائل بطريقة أنظف
    let amount = command
        .data
        .options
        .first()
        .and_then(|opt| opt.value.as_ref())
        .and_then(|val| val.as_i64())
        .map(|v| v.min(100).max(1) as usize)
        .unwrap_or(5);

    // جلب وحذف الرسائل باستخدام guard patterns
    match command
        .channel_id
        .messages(&ctx.http, |retriever| retriever.limit(amount as u64))
        .await
    {
        Ok(messages) if messages.is_empty() => {
            send_response(ctx, command, "❌ لا توجد رسائل لحذفها").await;
        }
        Ok(messages) => {
            match command.channel_id.delete_messages(&ctx.http, &messages).await {
                Ok(_) => {
                    let msg = format!("✅ تم حذف `{}` رسالة بنجاح!", messages.len());
                    send_response(ctx, command, &msg).await;
                }
                Err(why) => {
                    send_response(ctx, command, &format!("❌ خطأ في الحذف: {}", why)).await;
                }
            }
        }
        Err(why) => {
            send_response(ctx, command, &format!("❌ خطأ في جلب الرسائل: {}", why)).await;
        }
    }
}
