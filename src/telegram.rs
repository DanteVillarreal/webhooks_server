// src/telegram.rs

use teloxide::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::env;
use crate::{ create_openai_thread, first_loop, second_message_and_so_on, handle_message_handler, handle_audio_message, handle_voice_message};
use crate::{User, Chat, Audio, Voice};
use anyhow::anyhow;
use crate::Message as CustomMessage; // Alias your Message type to avoid name conflicts
//use teloxide::types::{ChatKind};



// Global HashMap to store user_id to thread_id mappings
lazy_static::lazy_static! {
    static ref USER_THREADS: Arc<Mutex<HashMap<u64, String>>> = Arc::new(Mutex::new(HashMap::new()));
}


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
        voice: message.voice().map(|voice| Voice {
            file_id: voice.file.id.clone(),  // Access the `file_id`
            file_unique_id: voice.file.unique_id.clone(),  // Access the `file_unique_id`
            duration: voice.duration as u64,
            file_size: Some(voice.file.size as u64),  // Convert and set file_size
            mime_type: voice.mime_type.as_ref().map(|mime| mime.to_string()),  // Convert Mime to String
            file_path: None,  // Initially None, to be fetched later
        }),
    }
}

pub async fn run_telegram_bot(pool: deadpool_postgres::Pool) {
    let bot = Bot::from_env();
    log::info!("Bot started");
    let openai_key = env::var("OPENAI_KEY").expect("OPENAI_KEY not set");
    let assistant_id = "asst_ybfxpPMxcuj7GZkwELR6sttt".to_string();
    
    teloxide::repl(bot.clone(), move |message: teloxide::prelude::Message, bot: Bot| {
        let openai_key = openai_key.clone();
        let assistant_id = assistant_id.clone();
        let bot_token = env::var("TELOXIDE_TOKEN").expect("TELOXIDE_TOKEN not set");

        let pool = pool.clone();

        async move {
            // Insert or update user in the database
            let result = async {
                // Extract user ID
                let user_id = message.from()
                    .ok_or_else(|| anyhow!("User not found in message"))
                    .map(|user| user.id.0 as i64)?;

                // Create user object
                let db_user = crate::DBUser {
                    id: user_id,
                    first_name: Some(message.from().unwrap().first_name.clone()),
                    last_name: message.from().unwrap().last_name.clone(),
                    username: message.from().unwrap().username.clone(),
                };

                // Insert or update user in the database
                if let Err(e) = crate::database::insert_user(pool.clone(), db_user).await {
                    log::error!("Failed to insert or update user: {:?}", e);
                }

                if let Some(text) = message.text() {
                    log::info!("Received message: {}", text);
                
                    // Initialize thread ID and handle message response
                    let thread_id_result = {
                        let mut user_threads = USER_THREADS.lock().await;
                        let maybe_thread_id = user_threads.get(&(user_id as u64)).cloned();
                
                        match maybe_thread_id {
                            Some(existing_thread_id) => {
                                existing_thread_id.parse::<i32>().map_err(|e| anyhow!("Failed to parse thread ID: {:?}", e))
                            },
                            None => {
                                match create_openai_thread(&openai_key, text).await {
                                    Ok(new_thread_id) => {
                                        user_threads.insert(user_id as u64, new_thread_id.clone());
                                        let thread_id_result = new_thread_id.parse::<i32>().map_err(|e| anyhow!("Failed to parse thread ID: {:?}", e));
                                        if let Ok(thread_id_value) = thread_id_result {
                                            if let Err(e) = crate::database::insert_thread(pool.clone(), thread_id_value, user_id, &new_thread_id).await {
                                                log::error!("Failed to insert or update thread: {:?}", e);
                                            }
                                        }
                                        thread_id_result
                                    },
                                    Err(e) => Err(anyhow!("Failed to create thread: {}", e)),
                                }
                            }
                        }
                    };
                
                    let thread_id = match thread_id_result {
                        Ok(id) => id,
                        Err(e) => {
                            log::error!("Text: Failed to get or create thread ID: {}", e);
                            return Ok(());
                        }
                    };
                
                    let response_result = second_message_and_so_on(&openai_key, &thread_id.to_string(), text, &assistant_id).await;
                
                    // Log the user's message
                    if let Err(e) = crate::database::insert_message(pool.clone(), thread_id, "user", text, "text").await {
                        log::error!("Failed to log user message: {:?}", e);
                    }
                
                    // Send the response back to the Telegram user and log it
                    match response_result {
                        Ok(response_value) => {
                            if let Err(e) = crate::database::insert_message(pool.clone(), thread_id, "assistant", &response_value, "text").await {
                                log::error!("Failed to log assistant message: {:?}", e);
                            }
                            bot.send_message(message.chat.id, response_value).await?;
                        },
                        Err(e) => {
                            log::error!("Failed to process message: {:?}", e);
                            bot.send_message(message.chat.id, "Failed to process message. Please try again later.").await?;
                        }
                    };
                } else if let Some(audio) = message.audio() {
                    log::info!("Received audio message");
                
                    let mut custom_message = convert_teloxide_message_to_custom(message.clone());
                    let chat_id = message.chat.id;
                
                    if let Some(ref mut custom_audio) = &mut custom_message.audio {
                        match get_file_path(&custom_audio.file_id, &bot_token).await {
                            Ok(file_path) => {
                                custom_audio.file_path = Some(file_path);
                                match handle_audio_message(&bot_token, &custom_audio, &openai_key).await {
                                    Ok(transcription) => {
                                        let user_id = message.from().map(|user| user.id.0).ok_or_else(|| anyhow!("User ID not found"))?;
                                        let thread_id_result = {
                                            let mut user_threads = USER_THREADS.lock().await;
                                            let maybe_thread_id = user_threads.get(&user_id).cloned();
                
                                            match maybe_thread_id {
                                                Some(existing_thread_id) => existing_thread_id.parse::<i32>().map_err(|e| anyhow!("Failed to parse thread ID: {:?}", e)),
                                                None => {
                                                    match create_openai_thread(&openai_key, &transcription).await {
                                                        Ok(new_thread_id) => {
                                                            user_threads.insert(user_id, new_thread_id.clone());
                                                            let thread_id_result = new_thread_id.parse::<i32>().map_err(|e| anyhow!("Failed to parse thread ID: {:?}", e));
                                                            if let Ok(thread_id_value) = thread_id_result {
                                                                if let Err(e) = crate::database::insert_thread(pool.clone(), thread_id_value, user_id as i64, &new_thread_id).await {
                                                                    log::error!("Failed to insert or update thread: {:?}", e);
                                                                }
                                                            }
                                                            thread_id_result
                                                        },
                                                        Err(e) => Err(anyhow!("Failed to create thread: {}", e)),
                                                    }
                                                }
                                            }
                                        };
                
                                        let thread_id = match thread_id_result {
                                            Ok(id) => id,
                                            Err(e) => {
                                                log::error!("Audio: Failed to get or create thread ID: {}", e);
                                                return Err(anyhow!("Failed to get or create thread"));
                                            }
                                        };
                
                                        if let Err(e) = crate::database::insert_message(pool.clone(), thread_id, "user", &transcription, "audio").await {
                                            log::error!("Failed to log user message: {:?}", e);
                                        }
                
                                        let response_result = second_message_and_so_on(&openai_key, &thread_id.to_string(), &transcription, &assistant_id).await;
                
                                        match response_result {
                                            Ok(response_value) => {
                                                if let Err(e) = crate::database::insert_message(pool.clone(), thread_id, "assistant", &response_value, "text").await {
                                                    log::error!("Failed to log assistant message: {:?}", e);
                                                }
                
                                                bot.send_message(chat_id, response_value).await?;
                                            },
                                            Err(e) => {
                                                log::error!("Failed to process message: {:?}", e);
                                                bot.send_message(chat_id, "Failed to process message. Please try again later.").await?;
                                            }
                                        }
                                    },
                                    Err(e) => {
                                        log::error!("Failed to handle audio message: {:?}", e);
                                        bot.send_message(chat_id, "Failed to process your audio message. Please try again later.").await?;
                                    }
                                }
                            },
                            Err(e) => {
                                log::error!("Failed to retrieve file path: {:?}", e);
                                bot.send_message(chat_id, "Failed to process your audio message. Please try again later.").await?;
                            }
                        }
                    }
                } else if let Some(voice) = message.voice() {
                    log::info!("Received voice message");

                    let mut custom_message = convert_teloxide_message_to_custom(message.clone());
                    let chat_id = message.chat.id;

                    if let Some(ref mut custom_voice) = &mut custom_message.voice {
                        match get_file_path(&custom_voice.file_id, &bot_token).await {
                            Ok(file_path) => {
                                custom_voice.file_path = Some(file_path);
                                match handle_voice_message(&bot_token, &custom_voice, &openai_key).await {
                                    Ok(transcription) => {
                                        let response;
                                        let mut thread_id: Result<i32, anyhow::Error> = Err(anyhow!("Missing thread ID"));
                                        let user_id = message.from().map(|user| user.id.0).ok_or_else(|| anyhow!("User ID not found"))?;

                                        {
                                            let mut user_threads = USER_THREADS.lock().await;
                                            let maybe_thread_id = user_threads.get(&user_id).cloned();

                                            match maybe_thread_id {
                                                Some(existing_thread_id) => {
                                                    thread_id = existing_thread_id.parse::<i32>().map_err(|e| anyhow!("Failed to parse thread ID: {:?}", e));
                                                    response = second_message_and_so_on(&openai_key, &existing_thread_id, &transcription, &assistant_id).await;
                                                },
                                                None => {
                                                    match create_openai_thread(&openai_key, &transcription).await {
                                                        Ok(new_thread_id) => {
                                                            user_threads.insert(user_id, new_thread_id.clone());
                                                            thread_id = new_thread_id.parse::<i32>().map_err(|e| anyhow!("Failed to parse thread ID: {:?}", e));
                                                            if let Ok(thread_id_value) = thread_id {
                                                                if let Err(e) = crate::database::insert_thread(pool.clone(), thread_id_value, user_id as i64, &new_thread_id).await {
                                                                    log::error!("Failed to insert or update thread: {:?}", e);
                                                                }
                                                            }
                                                            response = first_loop(&openai_key, &new_thread_id, &assistant_id).await;
                                                        },
                                                        Err(e) => {
                                                            log::error!("Failed to create thread: {}", e);
                                                            return Err(anyhow!("Failed to create thread"));
                                                        }
                                                    }
                                                }
                                            }
                                        }

                                        let thread_id_value = thread_id?;
                                        if let Err(e) = crate::database::insert_message(pool.clone(), thread_id_value, "user", &transcription, "voice").await {
                                            log::error!("Failed to log user message: {:?}", e);
                                        }

                                        match response {
                                            Ok(response) => {
                                                if let Err(e) = crate::database::insert_message(pool.clone(), thread_id_value, "assistant", &response, "text").await {
                                                    log::error!("Failed to log assistant message: {:?}", e);
                                                }

                                                bot.send_message(chat_id, response).await?;
                                            },
                                            Err(e) => {
                                                log::error!("Failed to process message: {}", e);
                                                bot.send_message(chat_id, "Failed to process message. Please try again later.").await?;
                                            }
                                        };
                                    },
                                    Err(e) => {
                                        log::error!("Failed to handle voice message: {:?}", e);
                                        bot.send_message(chat_id, "Failed to process your voice message. Please try again later.").await?;
                                    }
                                }
                            },
                            Err(e) => {
                                log::error!("Failed to retrieve file path: {:?}", e);
                                bot.send_message(chat_id, "Failed to process your voice message. Please try again later.").await?;
                            }
                        }
                    }
                }

                Ok(())
            }.await;

            // Handle any errors that were thrown during processing
            if let Err(error) = result {
                match &error.downcast_ref::<teloxide::RequestError>() {
                    Some(teloxide::RequestError::RetryAfter(duration)) => {
                        tokio::time::sleep(*duration).await;
                    },
                    Some(teloxide::RequestError::Api(api_error)) => {
                        log::error!("An error from the update listener: Api({})", api_error);
                    },
                    Some(teloxide::RequestError::Network(network_error)) => {
                        log::error!("An error from the update listener: Network({:?})", network_error);
                        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                    },
                    _ => {
                        log::error!("An unforeseen error from the update listener: {:?}", error);
                    }
                }
            }

            respond(())
        }
    }).await;
}




// pub async fn run_telegram_bot(pool: deadpool_postgres::Pool) {
//     let bot = Bot::from_env();
//     log::info!("Bot started");
//     let openai_key = env::var("OPENAI_KEY").expect("OPENAI_KEY not set");
//     let assistant_id = "asst_ybfxpPMxcuj7GZkwELR6sttt".to_string();
    
//     teloxide::repl(bot.clone(), move |message: teloxide::prelude::Message, bot: Bot| {
//         let openai_key = openai_key.clone();
//         let assistant_id = assistant_id.clone();
//         let bot_token = env::var("TELOXIDE_TOKEN").expect("TELOXIDE_TOKEN not set");

//         let pool = pool.clone();

//         async move {
//             if let Some(user) = message.from() {
//                 let db_user = crate::DBUser {
//                     id: user.id.0 as i64,
//                     first_name: Some(user.first_name.clone()),  
//                     last_name: user.last_name.clone(),
//                     username: user.username.clone(),
//                 };

//                 if let Err(e) = crate::database::insert_user(pool.clone(), db_user).await {
//                     log::error!("Failed to insert or update user: {:?}", e);
//                 }
//             }

//             let result = async {
//                 if let Some(text) = message.text() {
//                     log::info!("Received message: {}", text);

//                     let user_id = message.from().map(|user| user.id.0).unwrap_or(0);
//                     let mut thread_id: Result<i32, anyhow::Error> = Err(anyhow!("Missing thread ID"));

//                     let response = {
//                         let mut user_threads = USER_THREADS.lock().await;
//                         let maybe_thread_id = user_threads.get(&user_id).cloned();

//                         match maybe_thread_id {
//                             Some(existing_thread_id) => {
//                                 thread_id = existing_thread_id.parse::<i32>().map_err(|e| anyhow!("Failed to parse thread ID: {:?}", e));
//                                 second_message_and_so_on(&openai_key, &existing_thread_id, text, &assistant_id).await
//                             },
//                             None => {
//                                 match create_openai_thread(&openai_key, text).await {
//                                     Ok(new_thread_id) => {
//                                         user_threads.insert(user_id, new_thread_id.clone());
//                                         thread_id = new_thread_id.parse::<i32>().map_err(|e| anyhow!("Failed to parse thread ID: {:?}", e));
//                                         if let Err(e) = crate::database::insert_thread(pool.clone(), thread_id.clone()?, user_id, &new_thread_id).await {
//                                             log::error!("Failed to insert or update thread: {:?}", e);
//                                         }
//                                         first_loop(&openai_key, &new_thread_id, &assistant_id).await
//                                     },
//                                     Err(e) => {
//                                         log::error!("Failed to create thread: {}", e);
//                                         Err(anyhow!("Failed to create thread"))
//                                     }
//                                 }
//                             }
//                         }
//                     };

//                     let thread_id = thread_id?;
//                     if let Err(e) = crate::database::insert_message(pool.clone(), thread_id, "user", text, "text").await {
//                         log::error!("Failed to log user message: {:?}", e);
//                     }

//                     match response {
//                         Ok(response) => {
//                             if let Err(e) = crate::database::insert_message(pool.clone(), thread_id, "assistant", &response, "text").await {
//                                 log::error!("Failed to log assistant message: {:?}", e);
//                             }
                            
//                             bot.send_message(message.chat.id, response).await?;
//                         },
//                         Err(e) => {
//                             log::error!("Failed to process message: {}", e);
//                             bot.send_message(message.chat.id, "Failed to process message. Please try again later.").await?;
//                         }
//                     };
//                 } // Similar changes for audio and voice message handling underneath
//                 Ok(()) as Result<(), anyhow::Error>
//             }.await;

//             if let Err(error) = result {
//                 match &error.downcast_ref::<teloxide::RequestError>() {
//                     Some(teloxide::RequestError::RetryAfter(duration)) => {
//                         tokio::time::sleep(*duration).await;
//                     },
//                     Some(teloxide::RequestError::Api(api_error)) => {
//                         log::error!("An error from the update listener: Api({})", api_error);
//                     },
//                     Some(teloxide::RequestError::Network(network_error)) => {
//                         log::error!("An error from the update listener: Network({:?})", network_error);
//                         tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
//                     },
//                     _ => {
//                         log::error!("An unforeseen error from the update listener: {:?}", error);
//                     }
//                 }
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
    
//     teloxide::repl(bot.clone(), move |message: teloxide::prelude::Message, bot: Bot| {
//         let openai_key = openai_key.clone();
//         let assistant_id = assistant_id.clone();
//         let bot_token = env::var("TELOXIDE_TOKEN").expect("TELOXIDE_TOKEN not set");
    
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
//             } else if let Some(audio) = message.audio() {
//                 log::info!("Received audio message");
    
//                 let mut custom_message = convert_teloxide_message_to_custom(message.clone());  // Clone message for re-use
//                 let chat_id = message.chat.id;
    
//                 if let Some(ref mut custom_audio) = &mut custom_message.audio {
//                     match get_file_path(&custom_audio.file_id, &bot_token).await {
//                         Ok(file_path) => {
//                             custom_audio.file_path = Some(file_path);
//                             match handle_audio_message(&bot_token,  &custom_audio, &openai_key).await {
//                                 Ok(transcription) => {
//                                     // Now handle the transcription as if it was a text message from the user
//                                     let user_id = message.from().map(|user| user.id.0).unwrap_or(0);
//                                     let response = {
//                                         let mut user_threads = USER_THREADS.lock().await;
//                                         let maybe_thread_id = user_threads.get(&user_id).cloned();
    
//                                         match maybe_thread_id {
//                                             Some(existing_thread_id) => {
//                                                 second_message_and_so_on(&openai_key, &existing_thread_id, &transcription, &assistant_id).await
//                                             },
//                                             None => {
//                                                 match create_openai_thread(&openai_key, &transcription).await {
//                                                     Ok(new_thread_id) => {
//                                                         user_threads.insert(user_id, new_thread_id.clone());
//                                                         first_loop(&openai_key, &new_thread_id, &assistant_id).await
//                                                     },
//                                                     Err(e) => {
//                                                         log::error!("Failed to create thread: {}", e);
//                                                         Err(anyhow!("Failed to create thread"))
//                                                     }
//                                                 }
//                                             }
//                                         }
//                                     };
    
//                                     match response {
//                                         Ok(response) => {
//                                             bot.send_message(chat_id, response).await?;
//                                         },
//                                         Err(e) => {
//                                             log::error!("Failed to process message: {}", e);
//                                             bot.send_message(chat_id, "Failed to process message. Please try again later.").await?;
//                                         }
//                                     };
//                                 },
//                                 Err(e) => {
//                                     log::error!("Failed to handle audio message: {:?}", e);
//                                     bot.send_message(chat_id, "Failed to process your audio message. Please try again later.").await?;
//                                 }
//                             }
//                         },
//                         Err(e) => {
//                             log::error!("Failed to retrieve file path: {:?}", e);
//                             bot.send_message(chat_id, "Failed to process your audio message. Please try again later.").await?;
//                         }
//                     }
//                 }
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

//     teloxide::repl(bot.clone(), move |message: teloxide::prelude::Message, bot: Bot| {
//         let openai_key = openai_key.clone();
//         let assistant_id = assistant_id.clone();
//         let bot_token = env::var("TELOXIDE_TOKEN").expect("TELOXIDE_TOKEN not set");
    
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
//             } else if let Some(audio) = message.audio() {
//                 log::info!("Received audio message");
    
//                 let mut custom_message = convert_teloxide_message_to_custom(message.clone());  // Clone message for re-use
//                 let chat_id = message.chat.id;
    
//                 if let Some(ref mut custom_audio) = &mut custom_message.audio {
//                     match get_file_path(&custom_audio.file_id, &bot_token).await {
//                         Ok(file_path) => {
//                             custom_audio.file_path = Some(file_path);
//                             handle_message_handler(custom_message, openai_key.clone()).await;
//                             bot.send_message(chat_id, "Processing your audio message...").await?;
//                         },
//                         Err(e) => {
//                             log::error!("Failed to retrieve file path: {:?}", e);
//                             bot.send_message(chat_id, "Failed to process your audio message. Please try again later.").await?;
//                         }
//                     }
//                 }
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