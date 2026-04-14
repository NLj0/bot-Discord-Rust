use serenity::client::Context;
use serenity::model::application::CommandInteraction;
use serenity::builder::{CreateInteractionResponse, CreateInteractionResponseMessage, EditInteractionResponse};
use std::time::Instant;

pub async fn handle_ping(ctx: &Context, command: &CommandInteraction) {
    let start = Instant::now();
    
    // قياس وقت الكود قبل استدعاء API
    let code_time = start.elapsed().as_millis();

    // أرسل استجابة فارغة أولاً
    let _ = command.create_response(&ctx.http,
        CreateInteractionResponse::Message(
            CreateInteractionResponseMessage::new()
                .content("🏓 Pong!")
                .ephemeral(true)
        )
    ).await;
    
    let api_time = start.elapsed().as_millis() - code_time;
    let total_time = code_time + api_time;
    
    // تحديث الرسالة مع البيانات الصحيحة
    let _ = command.edit_response(&ctx.http,
        EditInteractionResponse::new().content(format!(
            "🏓 **Pong!**\n\
            ⚡ **سرعة الكود:** `{}ms`\n\
            📡 **سرعة API:** `{}ms`\n\
            🔄 **المجموع:** `{}ms`",
            code_time,
            api_time,
            total_time
        ))
    ).await;
}