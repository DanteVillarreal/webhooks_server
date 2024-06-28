// src/telegram.rs

use teloxide::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::env;
use crate::call_openai_api;

// pub async fn run_telegram_bot() {
//     let bot = Bot::from_env();
//     let user_assistant_map: Arc<Mutex<HashMap<u64, String>>> = Arc::new(Mutex::new(HashMap::new()));

//     let cloned_map = Arc::clone(&user_assistant_map);
//     teloxide::repl(bot, move |message: teloxide::types::Message, bot: Bot| {
//         let user_assistant_map = Arc::clone(&cloned_map);
//         async move {
//             if let Some(text) = message.text() {
//                 if text == "/list_assistants" {
//                     match list_assistants().await {
//                         Ok(response) => {
//                             bot.send_message(message.chat.id, response).await?;
//                         }
//                         Err(e) => {
//                             bot.send_message(message.chat.id, format!("Error: {:?}", e)).await?;
//                         }
//                     }
//                 } else if text.starts_with("/create_assistant") {
//                     let assistant_name = text.trim_start_matches("/create_assistant").trim();
//                     match create_assistant(assistant_name).await {
//                         Ok(response) => {
//                             bot.send_message(message.chat.id, response).await?;
//                         }
//                         Err(e) => {
//                             bot.send_message(message.chat.id, format!("Error: {:?}", e)).await?;
//                         }
//                     }
//                 } else if text.starts_with("/use_assistant") {
//                     if let Some(user_id) = message.from().map(|user| user.id) {
//                         let assistant_id = text.trim_start_matches("/use_assistant").trim().to_string();
//                         let mut map = user_assistant_map.lock().await;
//                         map.insert(user_id.0, assistant_id.clone());
//                         bot.send_message(message.chat.id, format!("Switched to assistant: {}", assistant_id)).await?;
//                     }
//                 } else {
//                     if let Some(user_id) = message.from().map(|user| user.id) {
//                         let map = user_assistant_map.lock().await;
//                         let assistant_id = map.get(&user_id.0).cloned().unwrap_or_else(|| "default_assistant_id".to_string());
//                         let response = call_openai_api(&assistant_id, text).await; // Use assistant_id here.
//                         bot.send_message(message.chat.id, response).await?;
//                     }
//                 }
//             }
//             respond(())
//         }
//     })
//     .await;
// }


use log::{info, error}; // Import logging macros

pub async fn run_telegram_bot() {
    let bot = Bot::from_env();
    info!("Bot started");
    let openai_key = env::var("OPENAI_KEY").expect("OPENAI_KEY not set");

    teloxide::repl(bot, move |message: teloxide::types::Message, bot: Bot| {
        let openai_key = openai_key.clone();
        async move {
            if let Some(text) = message.text() {
                info!("Received message: {}", text);

                // Call OpenAI API
                let response_text = call_openai_api(&openai_key, text).await;

                if let Err(e) = bot.send_message(
                    message.chat.id,
                    response_text,
                )
                .await {
                    error!("Error sending message: {}", e);
                } 
                else {
                    info!("Sent response to user");
                }
            } else {
                info!("Received a message without text");
            }
            respond(())
        }
    })
    .await;
}



async fn list_assistants() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let api_key = env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY not set");
    let client = reqwest::Client::new();

    let resp = client.get("https://api.openai.com/v1/assistants")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await?;

    let assistants = resp.json::<serde_json::Value>().await?;
    let mut assistant_list_str = String::new();

    if let Some(assistant_list) = assistants["data"].as_array() {
        for assistant in assistant_list {
            let id = assistant["id"].as_str().unwrap_or("No ID found");
            let name = assistant["name"].as_str().unwrap_or("No name found");
            assistant_list_str.push_str(&format!("Assistant ID: {}, Name: {}\n", id, name));
        }
    }

    Ok(assistant_list_str)
}

async fn create_assistant(name: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let api_key = env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY not set");
    let client = reqwest::Client::new();
    //WHY ARE WE USING V1
    let response = client.post("https://api.openai.com/v1/assistants")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&serde_json::json!({
            "name": name
        }))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    let assistant_id = response["id"].as_str().unwrap_or("");
    let assistant_name = response["name"].as_str().unwrap_or("");

    Ok(format!("Assistant created: ID: {}, Name: {}", assistant_id, assistant_name))
}