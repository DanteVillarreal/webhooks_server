// src/telegram.rs

use teloxide::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::env;
use crate::{ create_openai_thread, first_loop, second_message_and_so_on, handle_message_handler};
use crate::{User, Chat, Audio};
use anyhow::anyhow;
use crate::Message as CustomMessage; // Alias your Message type to avoid name conflicts
use teloxide::types::{ChatKind};

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
    static ref USER_THREADS: Arc<Mutex<HashMap<u64, String>>> = Arc::new(Mutex::new(HashMap::new()));
}

//use log::{info, error}; // Import logging macros

// pub async fn run_telegram_bot() {
//     let bot = Bot::from_env();
//     info!("Bot started");
//     let openai_key = env::var("OPENAI_KEY").expect("OPENAI_KEY not set");
//     let assistant_id = "asst_i3Rp5qhi8FtzZLBJ0Ibhr8ql".to_string();

//     teloxide::repl(bot.clone(), move |message: Message, bot: Bot| {
//         let openai_key = openai_key.clone();
//         let assistant_id = assistant_id.clone();

//         async move {
//             if let Some(text) = message.text() {
//                 info!("Received message: {}", text);

//                 // Use first_loop to handle the OpenAI interactions
//                 let response = match first_loop(&openai_key, text, &assistant_id).await {
//                     Ok(response) => response,
//                     Err(e) => {
//                         log::error!("Failed to process message: {}", e);
//                         "Failed to process message. Please try again later.".to_string()
//                     }
//                 };

//                 // Send the response back to the user
//                 if let Err(e) = bot.send_message(message.chat.id, response).await {
//                     log::error!("Failed to send message to Telegram: {}", e);
//                 }
//             }
//             respond(())
//         }
//     }).await;
// }
async fn get_file_path(file_id: &str, bot_token: &str) -> Result<String, anyhow::Error> {
    let url = format!("https://api.telegram.org/bot{}/getFile?file_id={}", bot_token, file_id);

    let client = reqwest::Client::new();
    let response = client.get(&url).send().await?;
    
    // Ensure the request was successful
    if !response.status().is_success() {
        anyhow::bail!("Received non-200 status code: {}", response.status());
    }

    let response_json: serde_json::Value = response.json().await?;
    if let Some(file_path) = response_json.get("result").and_then(|res| res.get("file_path")).and_then(|fp| fp.as_str()) {
        Ok(file_path.to_string())
    } else {
        anyhow::bail!("File path not found in response for file id: {}", file_id);
    }
}

fn convert_teloxide_message_to_custom(message: teloxide::prelude::Message) -> CustomMessage {
    CustomMessage {
        message_id: message.id.0 as u64,
        from: message.from().map(|user| User {
            id: user.id.0 as u64,
            is_bot: user.is_bot,
            first_name: Some(user.first_name.clone()),  // Wrapped in `Some`
            last_name: user.last_name.clone(),
            username: user.username.clone(),
        }),
        chat: Chat {
            id: message.chat.id.0 as u64,
            first_name: message.chat.first_name().map(|name| name.to_string()),  // Convert &str to String
            last_name: message.chat.last_name().map(|name| name.to_string()),    // Convert &str to String
            username: message.chat.username().map(|name| name.to_string()),      // Convert &str to String
            type_: match message.chat.kind {
                teloxide::types::ChatKind::Private(_) => "private".to_string(),
                _ => "other".to_string(),
            },
        },
        date: message.date.timestamp() as u64,  // Convert DateTime to UNIX timestamp
        text: message.text().map(|text| text.to_string()),
        audio: message.audio().map(|audio| Audio {
            file_id: audio.file.id.clone(),  // Access the `file_id`
            file_unique_id: audio.file.unique_id.clone(),  // Access the `file_unique_id`
            duration: audio.duration as u64,
            file_size: Some(audio.file.size as u64),  // Convert and set file_size
            file_path: None,  // Initially None, to be fetched later
            mime_type: audio.mime_type.as_ref().map(|mime| mime.to_string()),  // Convert Mime to String
            //mime_type: audio.mime_type.clone(), //doesnt work because is type mime
        }),
    }
}

pub async fn run_telegram_bot() {
    let bot = Bot::from_env();
    log::info!("Bot started");
    let openai_key = env::var("OPENAI_KEY").expect("OPENAI_KEY not set");
    let assistant_id = "asst_i3Rp5qhi8FtzZLBJ0Ibhr8ql".to_string();

    teloxide::repl(bot.clone(), move |message: teloxide::prelude::Message, bot: Bot| {
        let openai_key = openai_key.clone();
        let assistant_id = assistant_id.clone();
        let bot_token = env::var("TELOXIDE_TOKEN").expect("TELOXIDE_TOKEN not set");
    
        async move {
            if let Some(text) = message.text() {
                log::info!("Received message: {}", text);
    
                // Get user ID
                let user_id = message.from().map(|user| user.id.0).unwrap_or(0);
    
                // Async block to handle user-specific thread logic
                let response = {
                    let mut user_threads = USER_THREADS.lock().await;
                    let maybe_thread_id = user_threads.get(&user_id).cloned();
    
                    match maybe_thread_id {
                        Some(existing_thread_id) => {
                            // Use existing thread ID and process subsequent messages
                            second_message_and_so_on(&openai_key, &existing_thread_id, text, &assistant_id).await
                        },
                        None => {
                            // Create new thread and process the initial message
                            match create_openai_thread(&openai_key, text).await {
                                Ok(new_thread_id) => {
                                    user_threads.insert(user_id, new_thread_id.clone());
                                    first_loop(&openai_key, &new_thread_id, &assistant_id).await
                                },
                                Err(e) => {
                                    log::error!("Failed to create thread: {}", e);
                                    Err(anyhow!("Failed to create thread"))
                                }
                            }
                        }
                    }
                };
    
                // Send the response back to the Telegram user
                match response {
                    Ok(response) => {
                        bot.send_message(message.chat.id, response).await?;
                    },
                    Err(e) => {
                        log::error!("Failed to process message: {}", e);
                        bot.send_message(message.chat.id, "Failed to process message. Please try again later.").await?;
                    }
                };
            } else if let Some(audio) = message.audio() {
                log::info!("Received audio message");
    
                let mut custom_message = convert_teloxide_message_to_custom(message.clone());  // Clone message for re-use
                let chat_id = message.chat.id;
    
                if let Some(ref mut custom_audio) = &mut custom_message.audio {
                    match get_file_path(&custom_audio.file_id, &bot_token).await {
                        Ok(file_path) => {
                            custom_audio.file_path = Some(file_path);
                            handle_message_handler(custom_message, openai_key.clone()).await;
                            bot.send_message(chat_id, "Processing your audio message...").await?;
                        },
                        Err(e) => {
                            log::error!("Failed to retrieve file path: {:?}", e);
                            bot.send_message(chat_id, "Failed to process your audio message. Please try again later.").await?;
                        }
                    }
                }
            }
            respond(())
        }
    }).await;
}



// pub async fn run_telegram_bot() {
//     let bot = Bot::from_env();
//     log::info!("Bot started");
//     let openai_key = env::var("OPENAI_KEY").expect("OPENAI_KEY not set");
//     let assistant_id = "asst_i3Rp5qhi8FtzZLBJ0Ibhr8ql".to_string();

//     teloxide::repl(bot.clone(), move |message: Message, bot: Bot| {
//         let openai_key = openai_key.clone();
//         let assistant_id = assistant_id.clone();

//         async move {
//             if let Some(text) = message.text() {
//                 log::info!("Received message: {}", text);

//                 // Get user ID
//                 let user_id = message.from().map(|user| user.id.0).unwrap_or(0);

//                 // Async block to handle user-specific thread logic
//                 let response = {
//                     let mut user_threads = USER_THREADS.lock().await;
//                     let maybe_thread_id = user_threads.get(&user_id).cloned();

//                     match maybe_thread_id {
//                         Some(existing_thread_id) => {
//                             // Use existing thread ID and process subsequent messages
//                             second_message_and_so_on(&openai_key, &existing_thread_id, text, &assistant_id).await
//                         },
//                         None => {
//                             // Create new thread and process the initial message
//                             match create_openai_thread(&openai_key, text).await {
//                                 Ok(new_thread_id) => {
//                                     user_threads.insert(user_id, new_thread_id.clone());
//                                     first_loop(&openai_key, &new_thread_id, &assistant_id).await
//                                 },
//                                 Err(e) => {
//                                     log::error!("Failed to create thread: {}", e);
//                                     Err(anyhow!("Failed to create thread"))
//                                 }
//                             }
//                         }
//                     }
//                 };

//                 // Send the response back to the Telegram user
//                 match response {
//                     Ok(response) => {
//                         bot.send_message(message.chat.id, response).await?;
//                     },
//                     Err(e) => {
//                         log::error!("Failed to process message: {}", e);
//                         bot.send_message(message.chat.id, "Failed to process message. Please try again later.").await?;
//                     }
//                 };
//             }
//             respond(())
//         }
//     }).await;
// }



// pub async fn run_telegram_bot() {
//     let bot = Bot::from_env();
//     log::info!("Bot started");
//     let openai_key = env::var("OPENAI_KEY").expect("OPENAI_KEY not set");
//     let assistant_id = "asst_i3Rp5qhi8FtzZLBJ0Ibhr8ql".to_string();

//     teloxide::repl(bot.clone(), move |message: Message, bot: Bot| {
//         let openai_key = openai_key.clone();
//         let assistant_id = assistant_id.clone();

//         async move {
//             if let Some(text) = message.text() {
//                 let mut sentinel_value = 0;
//                 log::info!("Received message: {}", text);
//                 // let user_id: anyhow::Result<u64> = message.from()
//                 //     .map(|user| user.id.0)
//                 //     .ok_or_else(|| anyhow!(
//                 //         "User not found in the incoming message. Message details: chat_id={}, text={}",
//                 //         message.chat.id,
//                 //         text
//                 //     ));

//                 // let user_id = match user_id {
//                 //     Ok(id) => id,
//                 //     Err(err) => {
//                 //         bot.send_message(message.chat.id, err.to_string()).await?;
//                 //         return respond(());
//                 //     }
//                 // };
//                 log::info!("idk hello");
//                 let unused_var = first_loop(&openai_key, text, &assistant_id);
//                 // // Lock the global HashMap for thread safety
//                 // let mut user_threads = USER_THREADS.lock().await;

//                 // let (thread_id, mut run_id) = if let Some((thread_id, run_id)) = user_threads.get(&user_id) {
//                 //     (thread_id.clone(), run_id.clone())
//                 // } else {
//                 //     sentinel_value = 1;
//                 //     // Create a new thread
//                 //     let thread_id = match create_openai_thread(&openai_key, text).await {
//                 //         Ok(thread_id) => thread_id,
//                 //         Err(e) => {
//                 //             log::error!("Failed to create thread: {}", e);
//                 //             bot.send_message(message.chat.id, "Failed to create thread. Please try again later.").await?;
//                 //             return respond(());
//                 //         }
//                 //     };

//                 //     // Create a new run on the thread with the assistant
//                 //     let run_id = match create_run_on_thread(&openai_key, &thread_id, &assistant_id).await {
//                 //         Ok(run_id) => run_id,
//                 //         Err(e) => {
//                 //             log::error!("Failed to create run: {}", e);
//                 //             bot.send_message(message.chat.id, "Failed to create run. Please try again later.").await?;
//                 //             return respond(());
//                 //         }
//                 //     };

//                 //     // Store both thread_id and run_id in the map
//                 //     user_threads.insert(user_id, (thread_id.clone(), run_id.clone()));
//                 //     (thread_id, run_id)
//                 // };
//                 // // //A run is just a fucking "process message". that's it. 
//                 // // //I dont think we even need a hashmap of pairs of threads and runs
//                 // // //So everytime I need to send a message, I need to create another run.
//                 // // if sentinel_value == 0 {
//                 // //     //aka if it hasn't already made a run on the same message, then make a run
//                 // //     let _good = send_next_message(&thread_id, text);
//                 // //     run_id = match create_run_on_thread(&openai_key, &thread_id, &assistant_id).await {
//                 // //         Ok(run_id) => run_id,
//                 // //         Err(e) => {
//                 // //             log::error!("Failed to create run: {}", e);
//                 // //             bot.send_message(message.chat.id, "Failed to create run. Please try again later.").await?;
//                 // //             return respond(());
//                 // //         }
//                 // //     };
//                 // // }
//                 // // Send message within the run in the thread
//                 // match send_message_to_thread(&openai_key, &thread_id, &run_id, text).await {
//                 //     Ok(response) => {
//                 //         bot.send_message(message.chat.id, response).await?;
//                 //     }
//                 //     Err(e) => {
//                 //         log::error!("Error sending message to thread: {}", e);
//                 //         bot.send_message(message.chat.id, "Failed to send message. Please try again later.").await?;
//                 //     }
//                 // };
//             }
//             respond(())
//         }
//     }).await;
// }




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