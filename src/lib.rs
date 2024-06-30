// src/lib.rs

use serde::{Deserialize, Serialize};
use reqwest::Client;
use std::env;

use anyhow;

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

pub async fn create_openai_thread(openai_key: &str, initial_message: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::new();

    let response = client.post("https://api.openai.com/v1/threads")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", openai_key))
        .header("OpenAI-Beta", "assistants=v2")
        .json(&serde_json::json!({
            "messages": [{"role": "user", "content": initial_message}]
        }))
        .send()
        .await?;

    let response_text = response.text().await?;
    log::info!("Received response from create_openai_thread: {}", response_text);

    let response_json = serde_json::from_str::<serde_json::Value>(&response_text)?;
    let thread_id = response_json["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Thread ID not found in response"))?
        .to_string();
    log::info!("Created new thread with ID: {}", thread_id);
    Ok(thread_id)
}

pub async fn create_run_on_thread(openai_key: &str, thread_id: &str, assistant_id: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::new();

    // Payload with only assistant_id
    let json_payload = serde_json::json!({
        "assistant_id": assistant_id
    });

    log::info!("create_run_on_thread payload: {}", json_payload);

    let response = client.post(&format!("https://api.openai.com/v1/threads/{}/runs", thread_id))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", openai_key))
        .header("OpenAI-Beta", "assistants=v2")
        .json(&json_payload)
        .send()
        .await?;

    let response_text = response.text().await?;
    log::info!("Received response from create_run_on_thread: {}", response_text);

    let response_json: serde_json::Value = serde_json::from_str(&response_text)?;
    let run_id = response_json["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Run ID not found in response"))?
        .to_string();
    log::info!("Created new run with ID: {}", run_id);
    Ok(run_id)
}

pub async fn send_message_to_thread(openai_key: &str, thread_id: &str, message: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::new();

    let json_payload = serde_json::json!({
        "role": "user",
        "content": message
    });

    log::info!("send_message_to_thread payload: {}", json_payload);

    let response = client.post(&format!("https://api.openai.com/v1/threads/{}/messages", thread_id))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", openai_key))
        .header("OpenAI-Beta", "assistants=v2")
        .json(&json_payload)
        .send()
        .await?;

    let response_text = response.text().await?;
    log::info!("Received response from send_message_to_thread: {}", response_text);

    let response_json: serde_json::Value = serde_json::from_str(&response_text)?;

    // Extract the content text
    let response_content = response_json["content"]
        .as_array()
        .and_then(|content_array| content_array.get(0))
        .and_then(|content| content.get("text"))
        .and_then(|text| text.get("value"))
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow::anyhow!("Response content not found in response"))?
        .to_string();

    log::info!("lib.rs: Sent message to thread ID: {}, response: {}", thread_id, response_content);

    Ok(response_content)
}