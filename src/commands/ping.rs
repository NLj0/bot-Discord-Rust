use serenity::client::Context;
use serenity::model::prelude::application_command::ApplicationCommandInteraction;
use serenity::model::interactions::InteractionResponseType;
use serenity::model::interactions::InteractionApplicationCommandCallbackDataFlags;
use std::time::Instant;

pub async fn handle_ping(ctx: &Context, command: &ApplicationCommandInteraction) {
    let start = Instant::now();
    
    // قياس وقت الكود قبل استدعاء API
    let code_time = start.elapsed().as_millis();

    // أرسل استجابة فارغة أولاً
    let _ = command
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message| {
                    message
                        .content("🏓 Pong!")
                        .flags(InteractionApplicationCommandCallbackDataFlags::EPHEMERAL)
                })
        })
        .await;
    
    let api_time = start.elapsed().as_millis() - code_time;
    let total_time = code_time + api_time;
    
    // تحديث الرسالة مع البيانات الصحيحة
    let _ = command
        .edit_original_interaction_response(&ctx.http, |response| {
            response.content(format!(
                "🏓 **Pong!**\n\
                ⚡ **سرعة الكود:** `{}ms`\n\
                📡 **سرعة API:** `{}ms`\n\
                🔄 **المجموع:** `{}ms`",
                code_time,
                api_time,
                total_time
            ))
        })
        .await;
}