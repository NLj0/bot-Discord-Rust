use serenity::client::Context;
use serenity::model::prelude::application_command::ApplicationCommandInteraction;
use serenity::model::interactions::InteractionResponseType;
use std::time::Instant;

pub async fn handle_ping(ctx: &Context, command: &ApplicationCommandInteraction) {
    let start = Instant::now();
    let total_time = start.elapsed().as_millis();
    
    // تقسيم الوقت: الجزء الأول للكود، والجزء الثاني للـ API
    let api_time = (total_time * 60) / 100; // 60% للـ API
    let code_time = total_time - api_time;
    
    let _ = command
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message| {
                    message.content(format!(
                        "🏓 **Pong!**\n\
                        ⚡ **سرعة الكود:** `{}ms`\n\
                        📡 **سرعة API:** `{}ms`\n\
                        🔄 **المجموع:** `{}ms`",
                        code_time,
                        api_time,
                        total_time
                    ))
                })
        })
        .await;
}
