// src/lib.rs

use serde::{Deserialize, Serialize};
use reqwest::Client;
use std::env;

pub mod webhooks;
pub mod telegram;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WebhookPayload {
    pub update_id: u64,
    pub message: Option<Message>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Message {
    pub message_id: u64,
    pub from: Option<User>,
    pub chat: Chat,
    pub date: u64,
    pub text: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct User {
    pub id: u64,
    pub is_bot: bool,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub username: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Chat {
    pub id: u64,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub username: Option<String>,
    #[serde(rename = "type")]
    pub type_: String,
}

pub async fn handle_message(message: Message, input_text: String, openai_key: String) {
    let bot_token = env::var("TELOXIDE_TOKEN").expect("TELOXIDE_TOKEN not set");
    let chat_id = message.chat.id;

    let response_text = call_openai_api(&openai_key, &input_text).await;

    let bot = Client::new();
    bot.post(&format!("https://api.telegram.org/bot{}/sendMessage", bot_token))
        .json(&serde_json::json!({
            "chat_id": chat_id,
            "text": response_text,
        }))
        .send()
        .await
        .expect("Failed to send message to Telegram");
}

pub async fn call_openai_api(openai_key: &str, input: &str) -> String {
    let client = Client::new();

    let response = match client.post("https://api.openai.com/v1/chat/completions")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", openai_key))
        .json(&serde_json::json!({
            "model": "gpt-4o",
            "messages": [
                {
                    "role": "system",
                    "content": "You are a helpful assistant."
                },
                {
                    "role": "user",
                    "content": input
                }
            ]
        }))
        .send()
        .await {
            Ok(res) => res,
            Err(err) => {
                log::error!("Failed to send request to OpenAI: {:?}", err);
                return "Failed to get response from OpenAI".to_string();
            }
        };

    let response_json = match response.json::<serde_json::Value>().await {
        Ok(json) => json,
        Err(err) => {
            log::error!("Failed to parse response from OpenAI: {:?}", err);
            return "Failed to parse response from OpenAI".to_string();
        }
    };

    log::info!("OpenAI response: {:?}", response_json);

    response_json["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string()
}