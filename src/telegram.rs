// src/telegram.rs

use teloxide::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::env;
use crate::{call_openai_api, send_message_to_thread, create_openai_thread, create_run_on_thread};
use anyhow::anyhow;


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

// Global HashMap to store user_id to thread_id mappings
lazy_static::lazy_static! {
    static ref USER_THREADS: Arc<Mutex<HashMap<u64, (String, String)>>> = Arc::new(Mutex::new(HashMap::new()));
}

use log::{info, error}; // Import logging macros

pub async fn run_telegram_bot() {
    let bot = Bot::from_env();
    log::info!("Bot started");
    let openai_key = env::var("OPENAI_KEY").expect("OPENAI_KEY not set");
    let assistant_id = "asst_i3Rp5qhi8FtzZLBJ0Ibhr8ql".to_string(); // Your assistant ID as a String

    teloxide::repl(bot.clone(), move |message: Message, bot: Bot| {
        let openai_key = openai_key.clone();
        let assistant_id = assistant_id.clone();

        async move {
            if let Some(text) = message.text() {
                log::info!("Received message: {}", text);
                let user_id: anyhow::Result<u64> = message.from()
                    .map(|user| user.id.0)
                    .ok_or_else(|| anyhow!(
                        "User not found in the incoming message. Message details: chat_id={}, text={}",
                        message.chat.id,
                        text
                    ));

                let user_id = match user_id {
                    Ok(id) => id,
                    Err(err) => {
                        bot.send_message(message.chat.id, err.to_string()).await?;
                        return respond(());
                    }
                };

                // Lock the global HashMap for thread safety
                let mut user_threads = USER_THREADS.lock().await;

                let (thread_id, run_id) = if let Some((thread_id, run_id)) = user_threads.get(&user_id) {
                    (thread_id.clone(), run_id.clone())
                } else {
                    // Create a new thread
                    let thread_id = match create_openai_thread(&openai_key, text).await {
                        Ok(thread_id) => thread_id,
                        Err(e) => {
                            log::error!("Failed to create thread: {}", e);
                            bot.send_message(message.chat.id, "Failed to create thread. Please try again later.").await?;
                            return respond(());
                        }
                    };

                    // Create a new run on the thread with the assistant
                    let run_id = match create_run_on_thread(&openai_key, &thread_id, &assistant_id).await { // Removed text from parameters
                        Ok(run_id) => run_id,
                        Err(e) => {
                            log::error!("Failed to create run: {}", e);
                            bot.send_message(message.chat.id, "Failed to create run. Please try again later.").await?;
                            return respond(());
                        }
                    };

                    // Store both thread_id and run_id in the map
                    user_threads.insert(user_id, (thread_id.clone(), run_id.clone()));
                    (thread_id, run_id)
                };

                // Send message within the run in the thread
                match send_message_to_thread(&openai_key, &thread_id, &run_id, text).await {
                    Ok(response) => {
                        bot.send_message(message.chat.id, response).await?;
                    }
                    Err(e) => {
                        log::error!("Error sending message to thread: {}", e);
                        bot.send_message(message.chat.id, "Failed to send message. Please try again later.").await?;
                    }
                };
            }
            respond(())
        }
    }).await;
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