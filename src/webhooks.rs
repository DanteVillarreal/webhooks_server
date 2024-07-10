// src/webhooks.rs

use warp::Filter;
use crate::{WebhookPayload, handle_message_handler};
use std::env;

pub async fn run_webhook_server(pool: deadpool_postgres::Pool) {
    let openai_key = env::var("OPENAI_KEY").expect("OPENAI_KEY not set");

    // POST /webhook
    let webhook_route = warp::path("webhook")
        .and(warp::post())
        .and(warp::body::json())
        .and_then(move |payload: WebhookPayload| {
            let openai_key = openai_key.clone();
            async move {
                if let Some(ref message) = payload.message {
                    handle_message_handler(message.clone(), openai_key).await;
                }
                Ok::<_, warp::Rejection>(warp::reply::json(&"OK"))
            }
        });

    warp::serve(webhook_route)
        .run(([0, 0, 0, 0], 80))
        .await;
}