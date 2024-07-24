// src/webhooks.rs

use warp::Filter;
use crate::{WebhookPayload, handle_message_handler};
use std::env;
use teloxide::types::Message as TeloxideMessage;
use crate::telegram::convert_teloxide_message_to_custom;
use crate::Message as CustomMessage;

// pub async fn run_webhook_server(pool: deadpool_postgres::Pool) {
//     let openai_key = env::var("OPENAI_KEY").expect("OPENAI_KEY not set");

//     // POST /webhook
//     let webhook_route = warp::path("webhook")
//         .and(warp::post())
//         .and(warp::body::json())
//         .and_then(move |payload: WebhookPayload| {
//             let openai_key = openai_key.clone();
//             async move {
//                 if let Some(ref message) = payload.message {

                    
//                     // Store the custom message
//                     let mut messages = crate::telegram::NEW_MESSAGES.lock().await;
//                     messages.push(*message);
                
//                     // Handle the original message
//                     handle_message_handler(message.clone(), openai_key).await;
//                 }
//                 Ok::<_, warp::Rejection>(warp::reply::json(&"OK"))
//             }
//         });

//     warp::serve(webhook_route)
//         .run(([0, 0, 0, 0], 80))
//         .await;
// }




pub async fn run_webhook_server(pool: deadpool_postgres::Pool) {
    log::info!("IS CODE GETTING HEREEEEEEEEEEEEEEE");
    let openai_key = env::var("OPENAI_KEY").expect("OPENAI_KEY not set");

    // POST /webhook
    let webhook_route = warp::path("webhook")
        .and(warp::post())
        .and(warp::body::json())
        .and_then(move |payload: WebhookPayload| {
            let openai_key = openai_key.clone();
            async move {
                log::info!("Webhook called with payload: {:?}", payload);
                if let Some(ref message) = payload.message {
                    handle_message_handler(message.clone(), openai_key).await;
                }
                Ok::<_, warp::Rejection>(warp::reply::json(&"OK"))
            }
        });




    // GET /
        let html = tokio::fs::read_to_string("index.html").await.expect("Unable to read file");
        let html_route = warp::path::end()
            .map(move || warp::reply::html(html.clone()));

    
    // Combine routes:
        let routes = webhook_route.or(html_route);

    // Starts the warp server    
    warp::serve(routes)
        .run(([0, 0, 0, 0], 80))
        .await;
}