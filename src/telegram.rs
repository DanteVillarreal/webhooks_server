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
use crate::database::{insert_thread, insert_message};
//use teloxide::types::{ChatKind};
use tokio::time::{sleep, Duration};
use tokio::sync::RwLock;


// Global HashMap to store user_id to thread_id mappings
lazy_static::lazy_static! {
    static ref USER_THREADS: Arc<Mutex<HashMap<u64, String>>> = Arc::new(Mutex::new(HashMap::new()));
}

// Define UserState to store message buffer and timer
#[derive(Default)]
struct UserState {
    messages: Vec<crate::Message>,
    timer: Option<tokio::task::JoinHandle<()>>,
}
// Shared state to hold user states
type SharedState = Arc<RwLock<HashMap<u64, UserState>>>;

lazy_static::lazy_static! {
    static ref USER_STATES: SharedState = Arc::new(RwLock::new(HashMap::new()));
}





pub async fn get_file_path(file_id: &str, bot_token: &str) -> Result<String, anyhow::Error> {
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

pub fn convert_teloxide_message_to_custom(message: teloxide::prelude::Message) -> CustomMessage {
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



// pub async fn run_telegram_bot(pool: deadpool_postgres::Pool) {
//     let bot = teloxide::Bot::from_env();
//     log::info!("Bot started");
//     let openai_key = std::env::var("OPENAI_KEY").expect("OPENAI_KEY not set");
//     let assistant_id = "asst_ybfxpPMxcuj7GZkwELR6sttt".to_string();

//     teloxide::repl(bot.clone(), move |message: teloxide::prelude::Message, bot: teloxide::Bot| {
//         let openai_key = openai_key.clone();
//         let assistant_id = assistant_id.clone();
//         let bot_token = std::env::var("TELOXIDE_TOKEN").expect("TELOXIDE_TOKEN not set");
//         let pool = pool.clone();

//         async move {
//             let result: anyhow::Result<()> = async {
//                 let user_id = message.from()
//                     .ok_or_else(|| anyhow::anyhow!("User not found in message"))
//                     .map(|user| user.id.0 as i64)?;

//                 let db_user = crate::DBUser {
//                     id: user_id,
//                     first_name: Some(message.from().unwrap().first_name.clone()), // value from Telegram API, always Some
//                     last_name: Some(message.from().unwrap().last_name.clone().unwrap_or("N/A".to_string())), // convert None to "N/A"
//                     username: Some(message.from().unwrap().username.clone().unwrap_or("N/A".to_string())), // convert None to "N/A"
//                 };

//                 if let Err(e) = crate::database::insert_user(pool.clone(), db_user).await {
//                     log::error!("Failed to insert or update user: {:?}", e);
//                 }

//                 if let Some(text) = message.text() {
//                     log::info!("Received message: {}", text);

//                     // Cancel the existing timer if it exists
//                     let mut user_states = USER_STATES.write().await;
//                     let user_state = user_states.entry(user_id as u64).or_default();
//                     if let Some(timer) = user_state.timer.take() {
//                         timer.abort();
//                         log::info!("Existing timer canceled for user_id: {}", user_id);
//                     }

//                     // Buffer the new message
//                     user_state.messages.push(crate::telegram::convert_teloxide_message_to_custom(message.clone()));
//                     log::info!("Message buffered for user_id: {}", user_id);

//                     // Process buffered messages
//                     let (respond_cue, convo_response_text, convo_thread_id) =
//                         crate::telegram::handle_buffered_messages(
//                             user_id as u64, pool.clone(), bot.clone(), message.chat.id, openai_key.clone(), assistant_id.clone()
//                         ).await?;

//                     // Creating a new timer based on respond_cue
//                     if let Some(delay_seconds) = respond_cue {
//                         log::info!("Starting new {}-second timer for user_id: {}", delay_seconds, user_id);

//                         // Start a new timer
//                         let bot_clone = bot.clone();
//                         let pool_clone = pool.clone();
//                         user_state.timer = Some(tokio::spawn(async move {
//                             tokio::time::sleep(tokio::time::Duration::from_secs(delay_seconds as u64)).await;

//                             // Ensure we're not interrupted by new messages before sending the response
//                             let mut user_states = USER_STATES.write().await;
//                             if let Some(user_state) = user_states.get(&(user_id as u64)) {
//                                 if user_state.messages.is_empty() {
//                                     // Insert Convo AI response into the database and send to user
//                                     if let Err(e) = crate::database::insert_message(
//                                         pool_clone,
//                                         &convo_thread_id,
//                                         "assistant",
//                                         &convo_response_text,
//                                         "text",
//                                         &assistant_id,
//                                     ).await {
//                                         log::error!("Failed to log Convo AI response: {:?}", e);
//                                     }

//                                     bot_clone.send_message(message.chat.id, convo_response_text).await.ok();
//                                 } else {
//                                     log::info!("New message received before timer ended. Resetting process for user_id: {}", user_id);
//                                 }
//                             }
//                         }));
//                     }
                    

//                 }

//                 Ok(())
//             }.await;
//         } else if let Some(audio) = message.audio() {
//             log::info!("Received audio message");

//             // Cancel the existing timer if it exists
//             let mut user_states = crate::USER_STATES.write().await;
//             let user_state = user_states.entry(user_id as u64).or_default();
//             if let Some(timer) = user_state.timer.take() {
//                 timer.abort();
//                 log::info!("Existing timer canceled for user_id: {}", user_id);
//             }

//             // Buffer the audio message
//             user_state.messages.push(crate::telegram::convert_teloxide_message_to_custom(message.clone()));
//             log::info!("Audio message buffered for user_id: {}", user_id);

//             // Process buffered messages
//             let (respond_cue, convo_response_text, convo_thread_id) =
//                 crate::telegram::handle_buffered_messages(
//                     user_id as u64, pool.clone(), bot.clone(), message.chat.id, openai_key.clone(), assistant_id.clone()
//                 ).await?;

//             // Creating a new timer based on respond_cue
//             if let Some(delay_seconds) = respond_cue {
//                 log::info!("Starting new {}-second timer for user_id: {}", delay_seconds, user_id);

//                 // Start a new timer
//                 let bot_clone = bot.clone();
//                 let pool_clone = pool.clone();
//                 user_state.timer = Some(tokio::spawn(async move {
//                     tokio::time::sleep(tokio::time::Duration::from_secs(delay_seconds as u64)).await;

//                     // Ensure we're not interrupted by new messages before sending the response
//                     let mut user_states = crate::USER_STATES.write().await;
//                     if let Some(user_state) = user_states.get(&(user_id as u64)) {
//                         if user_state.messages.is_empty() {
//                             // Insert Convo AI response into the database and send to user
//                             if let Err(e) = crate::database::insert_message(
//                                 pool_clone,
//                                 &convo_thread_id,
//                                 "assistant",
//                                 &convo_response_text,
//                                 "text",
//                                 &assistant_id,
//                             ).await {
//                                 log::error!("Failed to log Convo AI response: {:?}", e);
//                             }

//                             bot_clone.send_message(message.chat.id, convo_response_text).await.ok();
//                         } else {
//                             log::info!("New message received before timer ended. Resetting process for user_id: {}", user_id);
//                         }
//                     }
//                 }));
//             }
//         } else {
//             // Handle other types of messages if needed
//         }

//         Ok(())
//     }.await;

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

//             teloxide::prelude::respond(())
//         }
//     }).await;
// }
pub async fn run_telegram_bot(pool: deadpool_postgres::Pool) {
    let bot = teloxide::Bot::from_env();
    log::info!("Bot started");
    let openai_key = std::env::var("OPENAI_KEY").expect("OPENAI_KEY not set");
    let assistant_id = "asst_ybfxpPMxcuj7GZkwELR6sttt".to_string();

    teloxide::repl(bot.clone(), move |message: teloxide::prelude::Message, bot: teloxide::Bot| {
        let openai_key = openai_key.clone();
        let assistant_id = assistant_id.clone();
        let bot_token = std::env::var("TELOXIDE_TOKEN").expect("TELOXIDE_TOKEN not set");

        let pool = pool.clone();

        async move {
            let result: anyhow::Result<()> = async {
                let user_id = message.from()
                    .ok_or_else(|| anyhow::anyhow!("User not found in message"))
                    .map(|user| user.id.0 as i64)?;

                let db_user = crate::DBUser {
                    id: user_id,
                    first_name: Some(message.from().unwrap().first_name.clone()), // value from Telegram API, always Some
                    last_name: Some(message.from().unwrap().last_name.clone().unwrap_or("N/A".to_string())), // convert None to "N/A"
                    username: Some(message.from().unwrap().username.clone().unwrap_or("N/A".to_string())), // convert None to "N/A"
                };

                if let Err(e) = crate::database::insert_user(pool.clone(), db_user).await {
                    log::error!("Failed to insert or update user: {:?}", e);
                }

                if let Some(text) = message.text() {
                    log::info!("Received message: {}", text);
                    let chat_id = message.chat.id;

                    let mut user_states = USER_STATES.write().await;
                    let user_state = user_states.entry(user_id as u64).or_default();

                    // Add message to buffer
                    user_state.messages.push(crate::telegram::convert_teloxide_message_to_custom(message.clone()));

                    // If there's an existing timer, cancel it
                    //IMPORTANT code comment explaining the code:
                    //tokio::spawn spawns an asynch task that run async - ok we know this
                    //then it RETURNS a JoinHandle. the "JoinHandle" is what allows us to control the spawned task
                    //this "JoinHandle" is then assigned to user_state.timer in user_state.timer = Some(tokio::spawn...). 
                    //So when run_telegram_bot() receives a new message, it goes through the logic and gets here:
                    //if let Some(timer) = user_state.timer.take() {
                    //     timer.abort();
                    // }
                    // user_state.timer is one of 2 things because of the code below: None or Some(JoinHandle<()>) .
                    //if let Some(timer) = user_state.timer.take() {} first turns 
                    //      user_state.timer to None, then says if
                    //      user_state.timer WAS a Some(something) and not None(),
                    //      aka before we did the take(), if user_state.timer
                    //      was a Some(T) and not None(),
                    //      let/set the new timer variable to something, aka JoinHandle<()>,
                    //      and do the code in {}. So in this context, the abort() method
                    //      is called on JoinHandle, which basically does what it says.
                    if let Some(timer) = user_state.timer.take() {
                        timer.abort();
                        log::info!("Existing timer reset for user_id: {}", user_id);
                    }

                    // Start a new timer
                    let bot_clone = bot.clone();
                    let openai_key_clone = openai_key.clone();
                    let assistant_id_clone = assistant_id.clone();
                    let pool_clone = pool.clone();

                    log::info!("Starting 15-second timer for user_id: {}", user_id);
                    user_state.timer = Some(tokio::spawn(async move {
                        log::info!("Waiting for any new messages for user_id: {}", user_id);
                        sleep(Duration::from_secs(15)).await;

                        // Handle the collected messages after the timeout
                        let result = handle_buffered_messages(user_id as u64, pool_clone, bot_clone, chat_id, openai_key_clone, assistant_id_clone).await;

                        match result {
                            Ok((response_cue, convo_response_text, convo_thread_id)) => {
                                let convo_response_text_clone = convo_response_text.clone();
                                if let Some(cue) = response_cue {
                                    // Now you can use `cue` as an `i32`
                                    log::info!("in run_telegram_bot: just finished out of handle_buffered_messages.
                                    respnonse cue timer initiating for {:?} seconds", &response_cue);
                                    let timer = cue + 30;
                                    sleep(Duration::from_secs(timer as u64)).await;
                                    //once done sleeping, insert the message into database...
                                    log::info!("in run_telegram_bot: inserting message into database");
                                    if let Err(e) = crate::database::insert_message(
                                        pool.clone(),
                                        &convo_thread_id,
                                        "assistant",
                                        &convo_response_text,
                                        "text",
                                        &assistant_id,).await 
                                    {
                                        log::error!("run_telegram_bot: Failed to log Convo AI response: {:?}", e);
                                    }
                                    //and send the message
                                    log::info!("sending convo response: {}", convo_response_text_clone);
                                    bot.send_message(chat_id, convo_response_text).await.ok();

                                } 
                                else {
                                    // Handle the case where `response_cue` is `None`
                                    log::error!("in run_telegram_bot: No response cue available.");
                                }
                                // Use `convo_response_text` and `convo_thread_id` as needed
                                //log::info!("convo Response text: {}", convo_response_text_clone);
                                log::info!("run_telegram_bot: Thread ID: {}", convo_thread_id);
                            }
                            Err(e) => {
                                // Handle the error
                                log::info!("run_telegram_bot: Error handling buffered messages: {:?}", e);
                            }
                        }

                        //CHANGE - DONEEEE:
                        //      make handle_buffered_messages return responce_cue, convo_response_text, convo_thread_id. 

                        //ADD:
                        //      timer with response_cue

                        //      bot.send_message convo_response_text
                        //      data insert message using thread id.
                    }));
                }

                // Handle audio messages
                // removed for now until I can handle audio and voice messages properly
                // else if let Some(audio) = message.audio() {
                //     log::info!("Received audio message");
                
                //     let mut custom_message = crate::telegram::convert_teloxide_message_to_custom(message.clone());
                //     let chat_id = message.chat.id;

                //     // Buffer the audio message
                //     let mut user_states = USER_STATES.write().await;
                //     let user_state = user_states.entry(user_id as u64).or_default();
                //     user_state.messages.push(custom_message);
                    
                //     // Reset the timer if it's active
                //     if let Some(timer) = user_state.timer.take() {
                //         timer.abort();
                //     }

                //     let bot_clone = bot.clone();
                //     let openai_key_clone = openai_key.clone();
                //     let assistant_id_clone = assistant_id.clone();
                //     let pool_clone = pool.clone();

                //     //IMPORTANT code comment explaining the code:
                //     //tokio::spawn spawns an asynch task that run async - ok we know this
                //     //then it RETURNS a JoinHandle. the "JoinHandle" is what allows us to control the spawned task
                //     //this "JoinHandle" is then assigned to user_state.timer in user_state.timer = Some(tokio::spawn...). 
                //     //So when run_telegram_bot() receives a new message, it goes through the logic and gets here:
                //     //if let Some(timer) = user_state.timer.take() {
                //     //     timer.abort();
                //     // }
                //     // user_state.timer is one of 2 things because of the code below: None or Some(JoinHandle<()>) .
                //     //if let Some(timer) = user_state.timer.take() {} first turns 
                //     //      user_state.timer to None, then says if
                //     //      user_state.timer WAS a Some(something) and not None(),
                //     //      aka before we did the take(), if user_state.timer
                //     //      was a Some(T) and not None(),
                //     //      let/set the new timer variable to something, aka JoinHandle<()>,
                //     //      and do the code in {}. So in this context, the abort() method
                //     //      is called on JoinHandle, which basically does what it says.
                //     user_state.timer = Some(tokio::spawn(async move {
                //         sleep(Duration::from_secs(15)).await;

                //         // Handle the collected messages after the timeout
                //         let (response_cue, convo_response_text, convo_thread_id) = handle_buffered_messages(user_id as u64, pool_clone, bot_clone, chat_id, openai_key_clone, assistant_id_clone).await;
                //     }));
                // }

                // // Handle voice messages
                // else if let Some(voice) = message.voice() {
                //     log::info!("Received voice message");
                
                //     let mut custom_message = crate::telegram::convert_teloxide_message_to_custom(message.clone());
                //     let chat_id = message.chat.id;

                //     // Buffer the voice message
                //     let mut user_states = USER_STATES.write().await;
                //     let user_state = user_states.entry(user_id as u64).or_default();
                //     user_state.messages.push(custom_message);
                    
                //     // Reset the timer if it's active
                //     if let Some(timer) = user_state.timer.take() {
                //         timer.abort();
                //     }

                //     let bot_clone = bot.clone();
                //     let openai_key_clone = openai_key.clone();
                //     let assistant_id_clone = assistant_id.clone();
                //     let pool_clone = pool.clone();

                //     user_state.timer = Some(tokio::spawn(async move {
                //         sleep(Duration::from_secs(15)).await;

                //         // Handle the collected messages after the timeout
                //         let (response_cue, convo_response_text, convo_thread_id) = handle_buffered_messages(user_id as u64, pool_clone, bot_clone, chat_id, openai_key_clone, assistant_id_clone).await;
                //     }));
                // }

                Ok(())
            }.await;

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

            teloxide::prelude::respond(())
        }
    }).await;
}
async fn handle_buffered_messages(
    user_id: u64,
    pool: deadpool_postgres::Pool,
    bot: teloxide::Bot,
    chat_id: teloxide::types::ChatId,
    openai_key: String,
    assistant_id: String,
) -> Result<(Option<i32>, String, String), anyhow::Error> {
    log::info!("about to acquire the write lock for USER_STATES");
    let mut user_states = USER_STATES.write().await;
    log::info!("acquired the write lock for USER_STATES");
    //  TODO: get user's message linked to the same assistant. because if we intercept the same uer's message but going to another assistant, 
    //      we dont want to concatenate THAT message too
    if let Some(user_state) = user_states.get_mut(&user_id) {
        log::info!("In handle_buffered_messages. Processing buffered messages for user_id: {}", user_id);

        // Concatenate all messages into a single string
        let concatenated_messages: String = user_state
            .messages
            .iter()
            .map(|message| message.text.clone().unwrap_or_default())
            .collect::<Vec<_>>()
            .join("\n");

        // Clear the user's message buffer
        user_state.messages.clear();
        log::info!("User message buffer cleared for user_id: {}", user_id);

        // Step 1: Pre-process the concatenated message with Analyzing AI.
        // Goal is to get response from Analyzing AI
        let analyzing_ai_id = "asst_JjoQ4OUjIgdhTgA9fiAIeRQu";
        // Step 1a: Send message to Analyzing AI to get/create a thread.
        let (analyzing_thread_id, is_new_thread) = crate::telegram::get_or_create_thread(&pool, user_id as i64, analyzing_ai_id, &openai_key, &concatenated_messages).await?;
        log::info!("in handle_buffed_messages. just got the new thread {}. is it new? {}", &analyzing_thread_id, &is_new_thread);
        // Step 1b: Run thread and receive response from Analyzing AI
        let response_text = if is_new_thread {
            crate::first_loop(&openai_key, &analyzing_thread_id, analyzing_ai_id).await?
        } else {
            crate::second_message_and_so_on(&openai_key, &analyzing_thread_id, &concatenated_messages, analyzing_ai_id).await?
        };

        log::info!("handle_buffered_messages: finished step 1b. ran thread and received response from Analyzing AI");


        // Step 2: Parse the Analyzing AI response
        let parsed_results = crate::parse_pre_processing_response(&response_text)?;

        // Step 4: Combine the original user message and parsed information into a final message
        let final_message = format!(
            "\n\nPre-processing results:\nQualified to Respond? {}\nInterest Level: {}\nRespond Cue: {:?}\nOriginal message:\n{}",
            parsed_results.qualified_to_respond,
            parsed_results.interest_level,
            parsed_results.respond_cue,
            concatenated_messages,
        );

        // Step 5: Process with Convo AI. Goal is to get response from Convo AI
        // Step 5a: Sending final_message to assistant's endpoint to get/create a thread
        let (convo_thread_id, is_new_thread) = crate::telegram::get_or_create_thread(&pool, user_id as i64, &assistant_id, &openai_key, &final_message).await?;

        // Step 5b: Insert user's concatenated message into the database.
        crate::database::insert_message(
            pool.clone(),
            &convo_thread_id,
            "assistant",
            &response_text,
            "text",
            analyzing_ai_id,
        ).await?;

        // Step 3: Insert parsed variables into the database.
        crate::database::insert_pre_processing_results(
            &pool,
            user_id,
            &convo_thread_id,
            parsed_results.interest_level,
            None,
            parsed_results.respond_cue,
        ).await?;

        // Step 5c: Run thread and receive response from Convo AI
        let convo_response_text = if is_new_thread {
            crate::first_loop(&openai_key, &convo_thread_id, &assistant_id).await?
        } else {
            crate::second_message_and_so_on(&openai_key, &convo_thread_id, &final_message, &assistant_id).await?
        };

        // Return the response cue, Convo AI response, and Convo thread ID
        return Ok((parsed_results.respond_cue, convo_response_text, convo_thread_id));
    }

    Err(anyhow::anyhow!("User state not found"))
}
//Replaced with above on 07/23/24 - because I want it to return response cue, convo AI response, and convo thread ID
// async fn handle_buffered_messages(
//     user_id: u64,
//     pool: deadpool_postgres::Pool,
//     bot: teloxide::Bot,
//     chat_id: teloxide::types::ChatId,
//     openai_key: String,
//     assistant_id: String,
// ) {
//     let mut user_states = USER_STATES.write().await;
//     if let Some(user_state) = user_states.get_mut(&user_id) {
//         // Concatenate all messages into a single string
//         let concatenated_messages: String = user_state
//             .messages
//             .iter()
//             .map(|message| message.text.clone().unwrap_or_default())
//             .collect::<Vec<_>>()
//             .join("\n");

//         // Clear the user's message buffer
//         user_state.messages.clear();

//         let thread_id = match crate::telegram::get_or_create_thread(&pool, user_id as i64, &assistant_id, &openai_key, &concatenated_messages).await {
//             Ok((thread_id, _)) => thread_id,
//             Err(e) => {
//                 log::error!("Failed to get or create thread: {:?}", e);
//                 bot.send_message(chat_id, "Failed to process messages. Please try again later.").await.ok();
//                 return;
//             }
//         };

//         if let Err(e) = crate::database::insert_message(pool.clone(), &thread_id, "user", &concatenated_messages, "text", &assistant_id).await {
//             log::error!("Failed to log user messages: {:?}", e);
//         }

//         // Process the concatenated messages
//         let response_result = match crate::telegram::second_message_and_so_on(&openai_key, &thread_id, &concatenated_messages, &assistant_id).await {
//             Ok(response_value) => {
//                 if let Err(e) = crate::database::insert_message(pool.clone(), &thread_id, "assistant", &response_value, "text", &assistant_id).await {
//                     log::error!("Failed to log assistant message: {:?}", e);
//                 }
//                 bot.send_message(chat_id, response_value).await.ok();
//                 Ok(())
//             },
//             Err(e) => {
//                 log::error!("Failed to process messages: {:?}", e);
//                 bot.send_message(chat_id, "Failed to process messages. Please try again later.").await.ok();
//                 Err(e)
//             }
//         };

//         if let Err(e) = response_result {
//             log::error!("Failed to send response: {:?}", e);
//         }
//     }
// }

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
//             let result: anyhow::Result<()> = async {
//                 let user_id = message.from()
//                     .ok_or_else(|| anyhow!("User not found in message"))
//                     .map(|user| user.id.0 as i64)?;
                
//                 let db_user = crate::DBUser {
//                     id: user_id,
//                     first_name: Some(message.from().unwrap().first_name.clone()), // value from Telegram API, always Some
//                     last_name: Some(message.from().unwrap().last_name.clone().unwrap_or("N/A".to_string())), // convert None to "N/A"
//                     username: Some(message.from().unwrap().username.clone().unwrap_or("N/A".to_string())), // convert None to "N/A"
//                 };
                
//                 if let Err(e) = crate::database::insert_user(pool.clone(), db_user).await {
//                     log::error!("Failed to insert or update user: {:?}", e);
//                 }

//                 // Handle user message text
// // Part of the `run_telegram_bot` function that handles text messages up to the point of handling `/summarize` command.

//                 if let Some(text) = message.text() {
//                     log::info!("Received message: {}", text);

//                     let chat_id = message.chat.id;
                    
//                     if text.trim().contains("/summarize") {
//                         log::info!("User requested to summarize conversation");
                        
//                         // Perform summarization only for this assistant ID
//                         let assistant_id = "asst_wjKt6A8SZxyywRtyHGSgbJu1";  // Target assistant ID for summarization
                        
//                         summarize_conversation(&pool, text, &openai_key, &bot, chat_id.0).await?;
                        
//                     } else {
//                         let (thread_id, is_new_thread) = get_or_create_thread(&pool, user_id, &assistant_id, &openai_key, text).await?;
                        
//                         if let Err(e) = insert_message(pool.clone(), &thread_id, "user", text, "text", &assistant_id).await {
//                             log::error!("Failed to log user message: {:?}", e);
//                         }
                        
//                         let response_result = if is_new_thread {
//                             first_loop(&openai_key, &thread_id, &assistant_id).await
//                         } else {
//                             second_message_and_so_on(&openai_key, &thread_id, text, &assistant_id).await
//                         };
                        
//                         match response_result {
//                             Ok(response_value) => {
//                                 introduce_delay().await;
//                                 if let Err(e) = insert_message(pool.clone(), &thread_id, "assistant", &response_value, "text", &assistant_id).await {
//                                     log::error!("Failed to log assistant message: {:?}", e);
//                                 }
//                                 bot.send_message(chat_id, response_value).await?;
//                             },
//                             Err(e) => {
//                                 log::error!("Failed to process message: {:?}", e);
//                                 bot.send_message(chat_id, "Failed to process message. Please try again later.").await?;
//                             }
//                         }
//                     }
//                 }

//                 // Handle audio messages
//                 else if let Some(audio) = message.audio() {
//                     log::info!("Received audio message");
                
//                     let mut custom_message = convert_teloxide_message_to_custom(message.clone());
//                     let chat_id = message.chat.id;
                
//                     let (thread_id, is_new_thread) = get_or_create_thread(&pool, user_id, &assistant_id, &openai_key, "Audio message initiated thread").await?;
                
//                     if let Some(ref mut custom_audio) = &mut custom_message.audio {
//                         match get_file_path(&custom_audio.file_id, &bot_token).await {
//                             Ok(file_path) => {
//                                 custom_audio.file_path = Some(file_path);
//                                 match handle_audio_message(&bot_token, &custom_audio, &openai_key).await {
//                                     Ok(transcription) => {
//                                         if let Err(e) = insert_message(pool.clone(), &thread_id, "user", &transcription, "audio", &assistant_id).await {
//                                             log::error!("Failed to log user message: {:?}", e);
//                                         }
                
//                                         let response_result = if is_new_thread {
//                                             first_loop(&openai_key, &thread_id, &assistant_id).await
//                                         } else {
//                                             second_message_and_so_on(&openai_key, &thread_id, &transcription, &assistant_id).await
//                                         };
                
//                                         match response_result {
//                                             Ok(response_value) => {
//                                                 introduce_delay().await;
//                                                 if let Err(e) = insert_message(pool.clone(), &thread_id, "assistant", &response_value, "text", &assistant_id).await {
//                                                     log::error!("Failed to log assistant message: {:?}", e);
//                                                 }
                
//                                                 bot.send_message(chat_id, response_value).await?;
//                                             },
//                                             Err(e) => {
//                                                 log::error!("Failed to process message: {:?}", e);
//                                                 bot.send_message(chat_id, "Failed to process message. Please try again later.").await?;
//                                             }
//                                         }
//                                     },
//                                     Err(e) => {
//                                         log::error!("Failed to handle audio message: {:?}", e);
//                                         bot.send_message(chat_id, "Failed to process your audio message. Please try again later.").await?;
//                                     }
//                                 }
//                             },
//                             Err(e) => {
//                                 log::error!("Failed to retrieve file path: {:?}", e);
//                                 bot.send_message(chat_id, "Failed to process your audio message. Please try again later.").await?;
//                             }
//                         }
//                     }
//                 }

//                 // Handle voice messages
//                 else if let Some(voice) = message.voice() {
//                     log::info!("Received voice message");
                
//                     let mut custom_message = convert_teloxide_message_to_custom(message.clone());
//                     let chat_id = message.chat.id;
                
//                     let (thread_id, is_new_thread) = get_or_create_thread(&pool, user_id, &assistant_id, &openai_key, "Voice message initiated thread").await?;
                
//                     if let Some(ref mut custom_voice) = &mut custom_message.voice {
//                         match get_file_path(&custom_voice.file_id, &bot_token).await {
//                             Ok(file_path) => {
//                                 custom_voice.file_path = Some(file_path);
//                                 match handle_voice_message(&bot_token, &custom_voice, &openai_key).await {
//                                     Ok(transcription) => {
//                                         if let Err(e) = insert_message(pool.clone(), &thread_id, "user", &transcription, "voice", &assistant_id).await {
//                                             log::error!("Failed to log user message: {:?}", e);
//                                         }
                
//                                         let response_result = if is_new_thread {
//                                             first_loop(&openai_key, &thread_id, &assistant_id).await
//                                         } else {
//                                             second_message_and_so_on(&openai_key, &thread_id, &transcription, &assistant_id).await
//                                         };
                
//                                         match response_result {
//                                             Ok(response_value) => {
//                                                 introduce_delay().await;
//                                                 if let Err(e) = insert_message(pool.clone(), &thread_id, "assistant", &response_value, "text", &assistant_id).await {
//                                                     log::error!("Failed to log assistant message: {:?}", e);
//                                                 }
                
//                                                 bot.send_message(chat_id, response_value).await?;
//                                             },
//                                             Err(e) => {
//                                                 log::error!("Failed to process message: {:?}", e);
//                                                 bot.send_message(chat_id, "Failed to process message. Please try again later.").await?;
//                                             }
//                                         }
//                                     },
//                                     Err(e) => {
//                                         log::error!("Failed to handle voice message: {:?}", e);
//                                         bot.send_message(chat_id, "Failed to process your voice message. Please try again later.").await?;
//                                     }
//                                 }
//                             },
//                             Err(e) => {
//                                 log::error!("Failed to retrieve file path: {:?}", e);
//                                 bot.send_message(chat_id, "Failed to process your voice message. Please try again later.").await?;
//                             }
//                         }
//                     }
//                 }

//                 Ok(())
//             }.await;

//             // Handle any errors that were thrown during processing
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

pub async fn get_or_create_thread(pool: &deadpool_postgres::Pool, user_id: i64, assistant_id: &str, openai_key: &str, initial_message: &str) -> Result<(String, bool), anyhow::Error> {
    let existing_thread_id = crate::database::get_thread_by_user_id_and_assistant(pool.clone(), user_id, assistant_id).await?;
    match existing_thread_id {
        Some(thread_id) => Ok((thread_id, false)),
        None => {
            let created_thread_id = create_openai_thread(openai_key, initial_message).await?;
            insert_thread(pool.clone(), &created_thread_id, user_id, &created_thread_id, assistant_id).await?;
            Ok((created_thread_id, true))
        },
    }
}

// async fn handle_buffered_messages(
//     user_id: u64,
//     pool: deadpool_postgres::Pool,
//     bot: teloxide::Bot,
//     chat_id: teloxide::types::ChatId,
//     openai_key: String,
//     assistant_id: String,
// ) -> Result<(Option<i32>, String, String), anyhow::Error> {
//     log::info!("about to acquire the write lock for USER_STATES");
//     let mut user_states = USER_STATES.write().await;
//     log::info!("acquired the write lock for USER_STATES");
//     //  TODO: get user's message linked to the same assistant. because if we intercept the same uer's message but going to another assistant, 
//     //      we dont want to concatenate THAT message too
//     if let Some(user_state) = user_states.get_mut(&user_id) {
//         log::info!("In handle_buffered_messages. Processing buffered messages for user_id: {}", user_id);

//         // Concatenate all messages into a single string
//         let concatenated_messages: String = user_state
//             .messages
//             .iter()
//             .map(|message| message.text.clone().unwrap_or_default())
//             .collect::<Vec<_>>()
//             .join("\n");

//         // Clear the user's message buffer
//         user_state.messages.clear();
//         log::info!("User message buffer cleared for user_id: {}", user_id);

//         // Step 1: Pre-process the concatenated message with Analyzing AI.
//         // Goal is to get response from Analyzing AI
//         let analyzing_ai_id = "asst_JjoQ4OUjIgdhTgA9fiAIeRQu";
//         // Step 1a: Send message to Analyzing AI to get/create a thread.
//         let (analyzing_thread_id, is_new_thread) = crate::telegram::get_or_create_thread(&pool, user_id as i64, analyzing_ai_id, &openai_key, &concatenated_messages).await?;
//         log::info!("in handle_buffed_messages. just got the new thread {}. is it new? {}", &analyzing_thread_id, &is_new_thread);
//         // Step 1b: Run thread and receive response from Analyzing AI
//         let response_text = if is_new_thread {
//             crate::first_loop(&openai_key, &analyzing_thread_id, analyzing_ai_id).await?
//         } else {
//             crate::second_message_and_so_on(&openai_key, &analyzing_thread_id, &concatenated_messages, analyzing_ai_id).await?
//         };

//         log::info!("handle_buffered_messages: finished step 1b. ran thread and received response from Analyzing AI");


//         // Step 2: Parse the Analyzing AI response
//         let parsed_results = crate::parse_pre_processing_response(&response_text)?;

//         // Step 4: Combine the original user message and parsed information into a final message
//         let final_message = format!(
//             "\n\nPre-processing results:\nQualified to Respond? {}\nInterest Level: {}\nRespond Cue: {:?}\nOriginal message:\n{}",
//             parsed_results.qualified_to_respond,
//             parsed_results.interest_level,
//             parsed_results.respond_cue,
//             concatenated_messages,
//         );

//         // Step 5: Process with Convo AI. Goal is to get response from Convo AI
//         // Step 5a: Sending final_message to assistant's endpoint to get/create a thread
//         let (convo_thread_id, is_new_thread) = crate::telegram::get_or_create_thread(&pool, user_id as i64, &assistant_id, &openai_key, &final_message).await?;

//         // Step 5b: Insert user's concatenated message into the database.
//         crate::database::insert_message(
//             pool.clone(),
//             &convo_thread_id,
//             "assistant",
//             &response_text,
//             "text",
//             analyzing_ai_id,
//         ).await?;

//         // Step 3: Insert parsed variables into the database.
//         crate::database::insert_pre_processing_results(
//             &pool,
//             user_id,
//             &convo_thread_id,
//             parsed_results.interest_level,
//             None,
//             parsed_results.respond_cue,
//         ).await?;

//         // Step 5c: Run thread and receive response from Convo AI
//         let convo_response_text = if is_new_thread {
//             crate::first_loop(&openai_key, &convo_thread_id, &assistant_id).await?
//         } else {
//             crate::second_message_and_so_on(&openai_key, &convo_thread_id, &final_message, &assistant_id).await?
//         };

//         // Return the response cue, Convo AI response, and Convo thread ID
//         return Ok((parsed_results.respond_cue, convo_response_text, convo_thread_id));
//     }

//     Err(anyhow::anyhow!("User state not found"))
// }










// async fn handle_text_message_logic(
//     message: teloxide::prelude::Message,
//     pool: deadpool_postgres::Pool,
//     bot: teloxide::Bot,
//     user_id: i64,
//     chat_id: teloxide::types::ChatId,
//     openai_key: String,
//     assistant_id: String,
// ) -> Result<(), anyhow::Error> {
//     log::info!("Received message: {}", message.text().unwrap_or_default());

//     let mut user_states = USER_STATES.write().await;
//     let user_state = user_states.entry(user_id as u64).or_default();

//     // Add message to the buffer
//     user_state.messages.push(crate::telegram::convert_teloxide_message_to_custom(message.clone()));

//     // If there is an existing timer, cancel it
//     if let Some(timer) = user_state.timer.take() {
//         log::info!("Existing timer cancelled for user_id: {}", user_id);
//         timer.abort();
//     }

//     log::info!("Starting timer for user_id: {}", user_id);

//     // Sleep for the intended duration
//     log::info!("About to sleep for 15 seconds for user_id: {}", user_id);
//     tokio::time::sleep(tokio::time::Duration::from_secs(15)).await;
//     log::info!("Timer expired for user_id: {}", user_id);

//     // Process buffered messages after the sleep duration
//     let (respond_cue, convo_response_text, convo_thread_id) = handle_buffered_messages(
//         user_id as u64,
//         pool.clone(),
//         bot.clone(),
//         chat_id,
//         openai_key.clone(),
//         assistant_id.clone(),
//     ).await?;

//     // Additional timer logic based on respond cue
//     if let Some(delay_seconds) = respond_cue {
//         log::info!("Starting new respond cue timer {}-second timer for user_id: {}", delay_seconds, user_id);
//         tokio::time::sleep(tokio::time::Duration::from_secs(delay_seconds as u64)).await;
//         let mut user_states = USER_STATES.write().await;
//         if let Some(user_state) = user_states.get(&(user_id as u64)) {
//             if user_state.messages.is_empty() {
//                 let pool_clone_inner2 = pool.clone(); // Clone again for further use
//                 let assistant_id_clone_inner2 = assistant_id.clone();
//                 if let Err(e) = crate::database::insert_message(
//                     pool_clone_inner2,
//                     &convo_thread_id,
//                     "assistant",
//                     &convo_response_text,
//                     "text",
//                     &assistant_id_clone_inner2,
//                 ).await {
//                     log::error!("Failed to log Convo AI response: {:?}", e);
//                 }
//                 bot.send_message(chat_id, convo_response_text).await.ok();
//             } else {
//                 log::info!("New message received before timer ended. Resetting process for user_id: {}", user_id);
//             }
//         }
//     }

//     Ok(())
// }


// async fn handle_audio_message_logic(
//     message: teloxide::prelude::Message, 
//     pool: deadpool_postgres::Pool,
//     bot: teloxide::Bot, 
//     user_id: i64, 
//     chat_id: teloxide::types::ChatId, 
//     openai_key: String,
//     assistant_id: String,
// ) -> Result<(), anyhow::Error> {
//     log::info!("Received audio message");

//     let mut custom_message = crate::telegram::convert_teloxide_message_to_custom(message.clone());

//     let mut user_states = USER_STATES.write().await;
//     let user_state = user_states.entry(user_id as u64).or_default();
//     user_state.messages.push(custom_message);

//     // If there's an existing timer, cancel it
//     if let Some(timer) = user_state.timer.take() {
//         timer.abort();
//         log::info!("Existing timer reset for user_id: {}", user_id);
//     }

//     // Handle the buffered messages, get the return values
//     let (respond_cue, convo_response_text, convo_thread_id) = handle_buffered_messages(
//         user_id as u64,
//         pool.clone(),
//         bot.clone(),
//         chat_id,
//         openai_key.clone(),
//         assistant_id.clone(),
//     ).await?;

//     // Creating a new timer based on respond_cue
//     if let Some(delay_seconds) = respond_cue {
//         log::info!("Starting new {}-second timer for user_id: {}", delay_seconds, user_id);
//         tokio::time::sleep(tokio::time::Duration::from_secs(delay_seconds as u64)).await;

//         let mut user_states = USER_STATES.write().await;
//         if let Some(user_state) = user_states.get(&(user_id as u64)) {
//             if user_state.messages.is_empty() {
//                 if let Err(e) = crate::database::insert_message(
//                     pool.clone(),
//                     &convo_thread_id,
//                     "assistant",
//                     &convo_response_text,
//                     "text",
//                     &assistant_id,
//                 ).await {
//                     log::error!("Failed to log Convo AI response: {:?}", e);
//                 }
//                 bot.send_message(chat_id, convo_response_text).await.ok();
//             } else {
//                 log::info!("New message received before timer ended. Resetting process for user_id: {}", user_id);
//                 return Ok(()); // Use return to exit early
//             }
//         }
//     }

//     Ok(())
// }

// async fn handle_voice_message_logic(
//     message: teloxide::prelude::Message,
//     pool: deadpool_postgres::Pool, 
//     bot: teloxide::Bot, 
//     user_id: i64, 
//     chat_id: teloxide::types::ChatId, 
//     openai_key: String, 
//     assistant_id: String,
// ) -> Result<(), anyhow::Error> {
//     log::info!("Received voice message");

//     let mut custom_message = crate::telegram::convert_teloxide_message_to_custom(message.clone());

//     let mut user_states = USER_STATES.write().await;
//     let user_state = user_states.entry(user_id as u64).or_default();
//     user_state.messages.push(custom_message);

//     // If there's an existing timer, cancel it
//     if let Some(timer) = user_state.timer.take() {
//         timer.abort();
//         log::info!("Existing timer reset for user_id: {}", user_id);
//     }

//     // Handle the buffered messages, get the return values
//     let (respond_cue, convo_response_text, convo_thread_id) = handle_buffered_messages(
//         user_id as u64,
//         pool.clone(),
//         bot.clone(),
//         chat_id,
//         openai_key.clone(),
//         assistant_id.clone(),
//     ).await?;

//     // Creating a new timer based on respond_cue
//     if let Some(delay_seconds) = respond_cue {
//         log::info!("Starting new {}-second timer for user_id: {}", delay_seconds, user_id);
//         tokio::time::sleep(tokio::time::Duration::from_secs(delay_seconds as u64)).await;

//         let mut user_states = USER_STATES.write().await;
//         if let Some(user_state) = user_states.get(&(user_id as u64)) {
//             if user_state.messages.is_empty() {
//                 if let Err(e) = crate::database::insert_message(
//                     pool.clone(),
//                     &convo_thread_id,
//                     "assistant",
//                     &convo_response_text,
//                     "text",
//                     &assistant_id,
//                 ).await {
//                     log::error!("Failed to log Convo AI response: {:?}", e);
//                 }
//                 bot.send_message(chat_id, convo_response_text).await.ok();
//             } else {
//                 log::info!("New message received before timer ended. Resetting process for user_id: {}", user_id);
//                 return Ok(()); // Use return to exit early
//             }
//         }
//     }

//     Ok(())
// }
// async fn get_or_create_thread(pool: &deadpool_postgres::Pool, user_id: i64, assistant_id: &str, openai_key: &str, initial_message: &str) -> Result<String, anyhow::Error> {
//     let existing_thread_id = crate::database::get_thread_by_user_id_and_assistant(pool.clone(), user_id, assistant_id).await?;
//     let thread_id = match existing_thread_id {
//         Some(thread_id) => thread_id,
//         None => {
//             match create_openai_thread(openai_key, initial_message).await {
//                 Ok(created_thread_id) => {
//                     insert_thread(pool.clone(), &created_thread_id, user_id, &created_thread_id, assistant_id).await?;
//                     first_loop(openai_key, &created_thread_id, assistant_id).await?;
//                     created_thread_id
//                 },
//                 Err(e) => {
//                     log::error!("Failed to create OpenAI thread: {:?}", e);
//                     return Err(e);
//                 }
//             }
//         }
//     };
//     Ok(thread_id)
// }




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
//             let result: anyhow::Result<()> = async {
//                 let user_id = message.from()
//                     .ok_or_else(|| anyhow!("User not found in message"))
//                     .map(|user| user.id.0 as i64)?;
                
//                 let db_user = crate::DBUser {
//                     id: user_id,
//                     first_name: Some(message.from().unwrap().first_name.clone()), // value from Telegram API, always Some
//                     last_name: Some(message.from().unwrap().last_name.clone().unwrap_or("N/A".to_string())), // convert None to "N/A"
//                     username: Some(message.from().unwrap().username.clone().unwrap_or("N/A".to_string())), // convert None to "N/A"
//                 };
                
//                 if let Err(e) = crate::database::insert_user(pool.clone(), db_user).await {
//                     log::error!("Failed to insert or update user: {:?}", e);
//                 }
                
//                 let thread_id;
//                 {
//                     let existing_thread_id = crate::database::get_thread_by_user_id_and_assistant(pool.clone(), user_id, &assistant_id).await?;
//                     thread_id = match existing_thread_id {
//                         Some(thread_id) => thread_id,
//                         None => {
//                             // If no existing thread, create a new one using first_loop
//                             if let Some(text) = message.text() {
//                                 log::info!("Handling first message with new thread creation.");
//                                 match create_openai_thread(&openai_key, text).await {
//                                     Ok(created_thread_id) => {
//                                         insert_thread(pool.clone(), &created_thread_id, user_id, &created_thread_id, &assistant_id).await?;
//                                         first_loop(&openai_key, &created_thread_id, &assistant_id).await?;
//                                         created_thread_id
//                                     },
//                                     Err(e) => {
//                                         log::error!("Failed to create OpenAI thread: {:?}", e);
//                                         return Ok(());
//                                     }
//                                 }
//                             } else {
//                                 log::error!("Received non-text message and no existing thread.");
//                                 return Ok(());
//                             }
//                         }
//                     };
//                 }

//                 // Handle user message text
//                 if let Some(text) = message.text() {
//                     log::info!("Received message: {}", text);
                
//                     // Check or create thread_id
//                     let thread_id;
//                     {
//                         let existing_thread_id = crate::database::get_thread_by_user_id_and_assistant(pool.clone(), user_id, &assistant_id).await?;
//                         thread_id = match existing_thread_id {
//                             Some(thread_id) => thread_id,
//                             None => {
//                                 log::info!("Handling first text message with new thread creation.");
//                                 match create_openai_thread(&openai_key, text).await {
//                                     Ok(created_thread_id) => {
//                                         insert_thread(pool.clone(), &created_thread_id, user_id, &created_thread_id, &assistant_id).await?;
//                                         first_loop(&openai_key, &created_thread_id, &assistant_id).await?;
//                                         created_thread_id
//                                     },
//                                     Err(e) => {
//                                         log::error!("Failed to create OpenAI thread: {:?}", e);
//                                         return Ok(());
//                                     }
//                                 }
//                             }
//                         };
//                     }
                
//                     if let Err(e) = insert_message(pool.clone(), &thread_id, "user", text, "text", &assistant_id).await {
//                         log::error!("Failed to log user message: {:?}", e);
//                     }
                
//                     let response_result = second_message_and_so_on(&openai_key, &thread_id, text, &assistant_id).await;
                
//                     match response_result {
//                         Ok(response_value) => {
//                             if let Err(e) = insert_message(pool.clone(), &thread_id, "assistant", &response_value, "text", &assistant_id).await {
//                                 log::error!("Failed to log assistant message: {:?}", e);
//                             }
//                             bot.send_message(message.chat.id, response_value).await?;
//                         },
//                         Err(e) => {
//                             log::error!("Failed to process message: {:?}", e);
//                             bot.send_message(message.chat.id, "Failed to process message. Please try again later.").await?;
//                         }
//                     }
//                 }
//                  // Audio and Voice handling remains unchanged
//                 else if let Some(audio) = message.audio() {
//                     log::info!("Received audio message");
                
//                     let mut custom_message = convert_teloxide_message_to_custom(message.clone());
//                     let chat_id = message.chat.id;
                    
//                     // Check or create thread_id
//                     let thread_id;
//                     {
//                         let existing_thread_id = crate::database::get_thread_by_user_id_and_assistant(pool.clone(), user_id, &assistant_id).await?;
//                         thread_id = match existing_thread_id {
//                             Some(thread_id) => thread_id,
//                             None => {
//                                 log::info!("Handling first audio message with new thread creation.");
//                                 match create_openai_thread(&openai_key, "Audio message initiated thread").await {
//                                     Ok(created_thread_id) => {
//                                         insert_thread(pool.clone(), &created_thread_id, user_id, &created_thread_id, &assistant_id).await?;
//                                         first_loop(&openai_key, &created_thread_id, &assistant_id).await?;
//                                         created_thread_id
//                                     },
//                                     Err(e) => {
//                                         log::error!("Failed to create OpenAI thread: {:?}", e);
//                                         return Ok(());
//                                     }
//                                 }
//                             }
//                         };
//                     }
                
//                     if let Some(ref mut custom_audio) = &mut custom_message.audio {
//                         match get_file_path(&custom_audio.file_id, &bot_token).await {
//                             Ok(file_path) => {
//                                 custom_audio.file_path = Some(file_path);
//                                 match handle_audio_message(&bot_token, &custom_audio, &openai_key).await {
//                                     Ok(transcription) => {
//                                         if let Err(e) = insert_message(pool.clone(), &thread_id, "user", &transcription, "audio", &assistant_id).await {
//                                             log::error!("Failed to log user message: {:?}", e);
//                                         }
                
//                                         let response_result = second_message_and_so_on(&openai_key, &thread_id, &transcription, &assistant_id).await;
                
//                                         match response_result {
//                                             Ok(response_value) => {
//                                                 if let Err(e) = insert_message(pool.clone(), &thread_id, "assistant", &response_value, "text", &assistant_id).await {
//                                                     log::error!("Failed to log assistant message: {:?}", e);
//                                                 }
                
//                                                 bot.send_message(chat_id, response_value).await?;
//                                             },
//                                             Err(e) => {
//                                                 log::error!("Failed to process message: {:?}", e);
//                                                 bot.send_message(chat_id, "Failed to process message. Please try again later.").await?;
//                                             }
//                                         }
//                                     },
//                                     Err(e) => {
//                                         log::error!("Failed to handle audio message: {:?}", e);
//                                         bot.send_message(chat_id, "Failed to process your audio message. Please try again later.").await?;
//                                     }
//                                 }
//                             },
//                             Err(e) => {
//                                 log::error!("Failed to retrieve file path: {:?}", e);
//                                 bot.send_message(chat_id, "Failed to process your audio message. Please try again later.").await?;
//                             }
//                         }
//                     }
//                 }
//                 else if let Some(voice) = message.voice() {
//                     log::info!("Received voice message");
                
//                     let mut custom_message = convert_teloxide_message_to_custom(message.clone());
//                     let chat_id = message.chat.id;
                
//                     // Check or create thread_id
//                     let thread_id;
//                     {
//                         let existing_thread_id = crate::database::get_thread_by_user_id_and_assistant(pool.clone(), user_id, &assistant_id).await?;
//                         thread_id = match existing_thread_id {
//                             Some(thread_id) => thread_id,
//                             None => {
//                                 log::info!("Handling first voice message with new thread creation.");
//                                 match create_openai_thread(&openai_key, "Voice message initiated thread").await {
//                                     Ok(created_thread_id) => {
//                                         insert_thread(pool.clone(), &created_thread_id, user_id, &created_thread_id, &assistant_id).await?;
//                                         first_loop(&openai_key, &created_thread_id, &assistant_id).await?;
//                                         created_thread_id
//                                     },
//                                     Err(e) => {
//                                         log::error!("Failed to create OpenAI thread: {:?}", e);
//                                         return Ok(());
//                                     }
//                                 }
//                             }
//                         };
//                     }
                
//                     if let Some(ref mut custom_voice) = &mut custom_message.voice {
//                         match get_file_path(&custom_voice.file_id, &bot_token).await {
//                             Ok(file_path) => {
//                                 custom_voice.file_path = Some(file_path);
//                                 match handle_voice_message(&bot_token, &custom_voice, &openai_key).await {
//                                     Ok(transcription) => {
//                                         if let Err(e) = insert_message(pool.clone(), &thread_id, "user", &transcription, "voice", &assistant_id).await {
//                                             log::error!("Failed to log user message: {:?}", e);
//                                         }
                
//                                         let response_result = second_message_and_so_on(&openai_key, &thread_id, &transcription, &assistant_id).await;
                
//                                         match response_result {
//                                             Ok(response_value) => {
//                                                 if let Err(e) = insert_message(pool.clone(), &thread_id, "assistant", &response_value, "text", &assistant_id).await {
//                                                     log::error!("Failed to log assistant message: {:?}", e);
//                                                 }
                
//                                                 bot.send_message(chat_id, response_value).await?;
//                                             },
//                                             Err(e) => {
//                                                 log::error!("Failed to process message: {:?}", e);
//                                                 bot.send_message(chat_id, "Failed to process message. Please try again later.").await?;
//                                             }
//                                         }
//                                     },
//                                     Err(e) => {
//                                         log::error!("Failed to handle voice message: {:?}", e);
//                                         bot.send_message(chat_id, "Failed to process your voice message. Please try again later.").await?;
//                                     }
//                                 }
//                             },
//                             Err(e) => {
//                                 log::error!("Failed to retrieve file path: {:?}", e);
//                                 bot.send_message(chat_id, "Failed to process your voice message. Please try again later.").await?;
//                             }
//                         }
//                     }
//                 }

//                 Ok(())
//             }.await;

//             // Handle any errors that were thrown during processing
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
//             let result = async {
//                 let user_id = message.from()
//                     .ok_or_else(|| anyhow!("User not found in message"))
//                     .map(|user| user.id.0 as i64)?;

//                 let db_user = crate::DBUser {
//                     id: user_id,
//                     first_name: Some(message.from().unwrap().first_name.clone()), // value from Telegram API, always Some
//                     last_name: Some(message.from().unwrap().last_name.clone().unwrap_or("N/A".to_string())), // convert None to "N/A"
//                     username: Some(message.from().unwrap().username.clone().unwrap_or("N/A".to_string())), // convert None to "N/A"
//                 };
                
//                 // Log the user details
//                 log::info!(
//                     "Preparing to insert user: id={} first_name={} last_name={} username={}",
//                     user_id,
//                     message.from().unwrap().first_name.clone(),
//                     message.from().unwrap().last_name.clone().unwrap_or("N/A".to_string()),
//                     message.from().unwrap().username.clone().unwrap_or("N/A".to_string()),
//                 );

//                 if let Err(e) = crate::database::insert_user(pool.clone(), db_user).await {
//                     log::error!("Failed to insert or update user: {:?}", e);
//                 }

//                 if let Some(text) = message.text() {
//                     log::info!("Received message: {}", text);

//                     let thread_id_result: Result<String, anyhow::Error>;
//                     {
//                         let mut user_threads = USER_THREADS.lock().await;
//                         let maybe_thread_id = user_threads.get(&(user_id as u64)).cloned();

//                         match maybe_thread_id {
//                             Some(existing_thread_id) => {
//                                 thread_id_result = Ok(existing_thread_id);
//                             },
//                             None => {
//                                 match create_openai_thread(&openai_key, text).await {
//                                     Ok(new_thread_id) => {
//                                         user_threads.insert(user_id as u64, new_thread_id.clone());
//                                         if let Err(e) = insert_thread(pool.clone(), &new_thread_id, user_id, &new_thread_id).await {
//                                             log::error!("Failed to insert or update thread: {:?}", e);
//                                         }
//                                         thread_id_result = Ok(new_thread_id);
//                                     },
//                                     Err(e) => {
//                                         thread_id_result = Err(anyhow!("Failed to create thread: {}", e));
//                                     },
//                                 }
//                             }
//                         }
//                     }

//                     let thread_id = match thread_id_result {
//                         Ok(id) => id,
//                         Err(e) => {
//                             log::error!("Text: Failed to get or create thread ID: {}", e);
//                             return Ok(());
//                         }
//                     };

//                     if let Err(e) = insert_message(pool.clone(), &thread_id, "user", text, "text").await {
//                         log::error!("Failed to log user message: {:?}", e);
//                     }

//                     let response_result = second_message_and_so_on(&openai_key, &thread_id, text, &assistant_id).await;

//                     match response_result {
//                         Ok(response_value) => {
//                             if let Err(e) = insert_message(pool.clone(), &thread_id, "assistant", &response_value, "text").await {
//                                 log::error!("Failed to log assistant message: {:?}", e);
//                             }
//                             bot.send_message(message.chat.id, response_value).await?;
//                         },
//                         Err(e) => {
//                             log::error!("Failed to process message: {:?}", e);
//                             bot.send_message(message.chat.id, "Failed to process message. Please try again later.").await?;
//                         }
//                     }
//                 } else if let Some(audio) = message.audio() {
//                     log::info!("Received audio message");
                
//                     let mut custom_message = convert_teloxide_message_to_custom(message.clone());
//                     let chat_id = message.chat.id;
                
//                     if let Some(ref mut custom_audio) = &mut custom_message.audio {
//                         match get_file_path(&custom_audio.file_id, &bot_token).await {
//                             Ok(file_path) => {
//                                 custom_audio.file_path = Some(file_path);
//                                 match handle_audio_message(&bot_token, &custom_audio, &openai_key).await {
//                                     Ok(transcription) => {
//                                         let thread_id_result: Result<String, anyhow::Error>;
//                                         {
//                                             let mut user_threads = USER_THREADS.lock().await;
//                                             let maybe_thread_id = user_threads.get(&(user_id as u64)).cloned();
                
//                                             match maybe_thread_id {
//                                                 Some(existing_thread_id) => {
//                                                     thread_id_result = Ok(existing_thread_id);
//                                                 },
//                                                 None => {
//                                                     match create_openai_thread(&openai_key, &transcription).await {
//                                                         Ok(new_thread_id) => {
//                                                             user_threads.insert(user_id as u64, new_thread_id.clone());
//                                                             if let Err(e) = insert_thread(pool.clone(), &new_thread_id, user_id, &new_thread_id).await {
//                                                                 log::error!("Failed to insert or update thread: {:?}", e);
//                                                             }
//                                                             thread_id_result = Ok(new_thread_id);
//                                                         },
//                                                         Err(e) => {
//                                                             thread_id_result = Err(anyhow!("Failed to create thread: {}", e));
//                                                         }
//                                                     }
//                                                 }
//                                             }
//                                         }
                
//                                         let thread_id = match thread_id_result {
//                                             Ok(id) => id,
//                                             Err(e) => {
//                                                 log::error!("Audio: Failed to get or create thread ID: {}", e);
//                                                 return Err(anyhow!("Failed to get or create thread"));
//                                             }
//                                         };
                
//                                         if let Err(e) = insert_message(pool.clone(), &thread_id, "user", &transcription, "audio").await {
//                                             log::error!("Failed to log user message: {:?}", e);
//                                         }
                
//                                         let response_result = second_message_and_so_on(&openai_key, &thread_id, &transcription, &assistant_id).await;
                
//                                         match response_result {
//                                             Ok(response_value) => {
//                                                 if let Err(e) = insert_message(pool.clone(), &thread_id, "assistant", &response_value, "text").await {
//                                                     log::error!("Failed to log assistant message: {:?}", e);
//                                                 }
                
//                                                 bot.send_message(chat_id, response_value).await?;
//                                             },
//                                             Err(e) => {
//                                                 log::error!("Failed to process message: {:?}", e);
//                                                 bot.send_message(chat_id, "Failed to process message. Please try again later.").await?;
//                                             }
//                                         }
//                                     },
//                                     Err(e) => {
//                                         log::error!("Failed to handle audio message: {:?}", e);
//                                         bot.send_message(chat_id, "Failed to process your audio message. Please try again later.").await?;
//                                     }
//                                 }
//                             },
//                             Err(e) => {
//                                 log::error!("Failed to retrieve file path: {:?}", e);
//                                 bot.send_message(chat_id, "Failed to process your audio message. Please try again later.").await?;
//                             }
//                         }
//                     }
//                 } else if let Some(voice) = message.voice() {
//                     log::info!("Received voice message");
                
//                     let mut custom_message = convert_teloxide_message_to_custom(message.clone());
//                     let chat_id = message.chat.id;
                
//                     if let Some(ref mut custom_voice) = &mut custom_message.voice {
//                         match get_file_path(&custom_voice.file_id, &bot_token).await {
//                             Ok(file_path) => {
//                                 custom_voice.file_path = Some(file_path);
//                                 match handle_voice_message(&bot_token, &custom_voice, &openai_key).await {
//                                     Ok(transcription) => {
//                                         let thread_id_result: Result<String, anyhow::Error>;
//                                         {
//                                             let mut user_threads = USER_THREADS.lock().await;
//                                             let maybe_thread_id = user_threads.get(&(user_id as u64)).cloned();
                
//                                             match maybe_thread_id {
//                                                 Some(existing_thread_id) => {
//                                                     thread_id_result = Ok(existing_thread_id);
//                                                 },
//                                                 None => {
//                                                     match create_openai_thread(&openai_key, &transcription).await {
//                                                         Ok(new_thread_id) => {
//                                                             user_threads.insert(user_id as u64, new_thread_id.clone());
//                                                             if let Err(e) = insert_thread(pool.clone(), &new_thread_id, user_id, &new_thread_id).await {
//                                                                 log::error!("Failed to insert or update thread: {:?}", e);
//                                                             }
//                                                             thread_id_result = Ok(new_thread_id);
//                                                         },
//                                                         Err(e) => {
//                                                             thread_id_result = Err(anyhow!("Failed to create thread: {}", e));
//                                                         }
//                                                     }
//                                                 }
//                                             }
//                                         }
                
//                                         let thread_id = match thread_id_result {
//                                             Ok(id) => id,
//                                             Err(e) => {
//                                                 log::error!("Voice: Failed to get or create thread ID: {}", e);
//                                                 return Err(anyhow!("Failed to get or create thread"));
//                                             }
//                                         };
                
//                                         if let Err(e) = insert_message(pool.clone(), &thread_id, "user", &transcription, "voice").await {
//                                             log::error!("Failed to log user message: {:?}", e);
//                                         }
                
//                                         let response_result = second_message_and_so_on(&openai_key, &thread_id, &transcription, &assistant_id).await;
                
//                                         match response_result {
//                                             Ok(response_value) => {
//                                                 if let Err(e) = insert_message(pool.clone(), &thread_id, "assistant", &response_value, "text").await {
//                                                     log::error!("Failed to log assistant message: {:?}", e);
//                                                 }
                
//                                                 bot.send_message(chat_id, response_value).await?;
//                                             },
//                                             Err(e) => {
//                                                 log::error!("Failed to process message: {:?}", e);
//                                                 bot.send_message(chat_id, "Failed to process message. Please try again later.").await?;
//                                             }
//                                         }
//                                     },
//                                     Err(e) => {
//                                         log::error!("Failed to handle voice message: {:?}", e);
//                                         bot.send_message(chat_id, "Failed to process your voice message. Please try again later.").await?;
//                                     }
//                                 }
//                             },
//                             Err(e) => {
//                                 log::error!("Failed to retrieve file path: {:?}", e);
//                                 bot.send_message(chat_id, "Failed to process your voice message. Please try again later.").await?;
//                             }
//                         }
//                     }
//                 }

//                 Ok(())
//             }.await;

//             // Handle any errors that were thrown during processing
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