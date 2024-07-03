// src/lib.rs

use serde::{Deserialize, Serialize};
use reqwest::Client;
use std::env;
use uuid::Uuid;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use anyhow;
use reqwest::multipart;

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
    pub audio: Option<Audio>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Audio {
    pub file_id: String,
    //pub file_unique_id: String,
    pub duration: u64,
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











pub async fn handle_message_handler(message: Message, openai_key: String,) {
    log::info!("Audio: step 1: Got to handle message handler fn");
    match handle_message(message.clone(), openai_key.clone()).await {
        Ok(_) => (),
        Err(e) => log::error!("Error handling message: {:?}", e),
    }
}

pub async fn handle_message(message: Message, openai_key: String, ) -> Result<(), anyhow::Error> {
    log::info!("god to handle_message fn");
    let bot_token = env::var("TELOXIDE_TOKEN")
        .expect("TELOXIDE_TOKEN does not exist. check naming");
    let chat_id = message.chat.id;

    if let Some(ref text) = message.text {
        log::info!("about to handle message as a text");
        handle_text_message(&bot_token, &chat_id, text, &openai_key).await?;
    } else if let Some(ref audio) = message.audio {
        log::info!("about to handle message as audio");
        handle_audio_message(&bot_token, &chat_id, audio, &openai_key).await?;
    }

    Ok(())
}

async fn handle_text_message(bot_token: &str, chat_id: &u64, input_text: &str, openai_key: &str) -> Result<(), anyhow::Error> {
    let response_text = call_openai_api(openai_key, input_text).await;

    let bot = Client::new();
    bot.post(&format!("https://api.telegram.org/bot{}/sendMessage", bot_token))
        .json(&serde_json::json!({
            "chat_id": chat_id,
            "text": response_text,
        }))
        .send()
        .await?;

    Ok(())
}

async fn handle_audio_message(bot_token: &str, chat_id: &u64, audio: &Audio, openai_key: &str) -> Result<(), anyhow::Error> {
    // Download the audio file from Telegram
    let file_url = format!("https://api.telegram.org/file/bot{}/{}", bot_token, audio.file_id);
    let file_path = download_file(&file_url, "audio").await?;

    // Call OpenAI API to transcribe audio
    let transcription = transcribe_audio(openai_key, &file_path).await?;

    let bot = Client::new();
    bot.post(&format!("https://api.telegram.org/bot{}/sendMessage", bot_token))
        .json(&serde_json::json!({
            "chat_id": chat_id,
            "text": transcription,
        }))
        .send()
        .await?;

    Ok(())
}

async fn download_file(url: &str, file_type: &str) -> Result<String, anyhow::Error> {
    let client = Client::new();
    let response = client.get(url).send().await?;
    
    let filename = format!("{}.{}", Uuid::new_v4(), file_type);
    let mut file = File::create(&filename).await?;
    let content = response.bytes().await?;
    file.write_all(&content).await?;

    Ok(filename)
}

async fn transcribe_audio(
    openai_key: &str,
    file_path: &str,
) -> Result<String, anyhow::Error> {
    let client = Client::new();

    // Read the content of the file
    let mut file = tokio::fs::File::open(file_path).await?;
    let mut file_content = Vec::new();
    file.read_to_end(&mut file_content).await?;

    let file_part = multipart::Part::stream(file_content);
    let form = multipart::Form::new()
        .text("model", "whisper-1")
        .part("file", file_part);

    let response = client
        .post("https://api.openai.com/v1/audio/transcriptions")
        .header("Authorization", format!("Bearer {}", openai_key))
        .multipart(form)
        .send()
        .await?;

    let response_json: serde_json::Value = response.json().await?;
    let transcription = response_json["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Transcription not found in response"))?
        .to_string();

    Ok(transcription)
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
    log::info!("Step 2 starting.In create_openai_thread rn. ");
    log::info!("Step 3 technically starting as well since we are using the message");
    log::info!("  in the json payload in the POST request to the url");
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
    log::info!("Step 2 initiating. aka POST https://api.openai.com/v1/threads");
    log::info!("Received response from create_openai_thread: {}", response_text);

    let response_json = serde_json::from_str::<serde_json::Value>(&response_text)?;
    let thread_id = response_json["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Thread ID not found in response"))?
        .to_string();
    log::info!("Created new thread with ID: {}", thread_id);
    log::info!("Step 2 complete. And step 3 as well");
    Ok(thread_id)
}

pub async fn create_run_on_thread(openai_key: &str, thread_id: &str, assistant_id: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::new();

    // Payload with only assistant_id
    let json_payload = serde_json::json!({
        "assistant_id": assistant_id
    });
    log::info!("Now in step 4's function: create_run_on_thread");
    log::info!("create_run_on_thread payload: {}", json_payload);

    let response = client.post(&format!("https://api.openai.com/v1/threads/{}/runs", thread_id))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", openai_key))
        .header("OpenAI-Beta", "assistants=v2")
        .json(&json_payload)
        .send()
        .await?;

    let response_text = response.text().await?;
    log::info!("Step 4 starting. aka POST https://api.openai.com/v1/threads/{thread_id}/runs");
    log::info!("Received response from create_run_on_thread: {}", response_text);

    let response_json: serde_json::Value = serde_json::from_str(&response_text)?;
    let run_id = response_json["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Run ID not found in response"))?
        .to_string();
    log::info!("Created new run with ID: {}", run_id);
    log::info!("Step 4 complete");
    Ok(run_id)
}

pub async fn is_run_active(openai_key: &str, thread_id: &str, run_id: &str) -> anyhow::Result<bool> {
    let client = reqwest::Client::new();
    let url = format!("https://api.openai.com/v1/threads/{}/runs/{}", thread_id, run_id);

    log::info!("Step 5 initiating. Aka Checking run's status to see if it's done");
    log::info!("AKA GET https://api.openai.com/v1/threads/{thread_id}/runs/{run_id}");

    let response = client.get(&url)
        .header("Authorization", format!("Bearer {}", openai_key))
        .header("OpenAI-Beta", "assistants=v2")
        .send()
        .await?;

    let response_text = response.text().await?;
    log::info!("Received response from is_run_active: {}", response_text);

    let response_json: serde_json::Value = serde_json::from_str(&response_text)?;

    let status = response_json["status"].as_str().unwrap_or("");
    
    Ok(status == "queued" || status == "started" || status == "in_progress")
}



pub async fn get_last_assistant_message(openai_key: &str, thread_id: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::new();

    log::info!("Step 6 initiating. Aka: Retrieve the assistant's response");
    log::info!("AKA: GET https://api.openai.com/v1/threads/{thread_id}/messages");

    let response = client.get(&format!("https://api.openai.com/v1/threads/{}/messages", thread_id))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", openai_key))
        .header("OpenAI-Beta", "assistants=v2") // Added the missing header
        .send()
        .await?;

    let response_text = response.text().await?;
    log::info!("Step 6 complete.");
    log::info!("Received response from get_last_assistant_message: {}", response_text);

    let response_json: serde_json::Value = serde_json::from_str(&response_text)?;

    // Extract the data array
    let data = response_json["data"].as_array().unwrap();
    // Filter out the assistant's messages
    let assistant_messages: Vec<&serde_json::Value> = data.iter().filter(|msg| msg["role"].as_str() == Some("assistant")).collect();
    // Extract the content text of the assistant's messages
    let assistant_texts: Vec<String> = assistant_messages.iter().map(|msg| {
        msg["content"][0]["text"]["value"].as_str().unwrap().to_string()
    }).collect();
    if let Some(last_message) = assistant_texts.first() {
        log::info!("The last message from the assistant is: {}", last_message);
        Ok(last_message.to_string())
    } else {
        log::error!("The assistant has not sent any messages.");
        Ok("No assistant response found".to_string())
    }
    

    // // Iterate over the messages in reverse to find the last assistant message
    // let messages = response_json["messages"].as_array().ok_or_else(|| anyhow::anyhow!("Messages array not found"))?;
    // for message in messages.iter().rev() {
    //     log::info!("SEEING IF THIS IS READ");
    //     if message["role"] == "assistant" {
    //         // Return the assistant's message content
    //         log::info!("found the assistant's message!");
    //         return Ok(message["content"][0]["text"]["value"].as_str().unwrap_or("").to_string());
    //     }
    // }
    // log::error!("didn't find the asisstant's message");
    // Ok("No assistant response found".to_string())
}

//Removed: 07/01/24 - ineffective.
// pub async fn send_message_to_thread(openai_key: &str, thread_id: &str, run_id: &str, message: &str) -> anyhow::Result<String> {
//     let client = reqwest::Client::new();

//     let json_payload = serde_json::json!({
//         "role": "user",
//         "content": message
//     });

//     log::info!("send_message_to_thread payload: {}", json_payload);

//     // Retry logic added
//     const MAX_RETRIES: u32 = 5;
//     for attempt in 0..MAX_RETRIES {
//         // Check if the run is active
//         if is_run_active(openai_key, thread_id, run_id).await? {
//             log::warn!("Active run detected, retrying... Attempt {}", attempt + 1);
//             tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
//             continue;
//         }
//         log::info!("Step 5 complete. AKA run status has been checked. NOT ACTIVE\n");
//         log::info!("Step 3 initiating. Adding a Users' message to the thread.");
//         log::info!("Aka POST https://api.openai.com/v1/threads/{thread_id}/messages");
//         // Send user message
//         let response = client.post(&format!("https://api.openai.com/v1/threads/{}/messages", thread_id))
//             .header("Content-Type", "application/json")
//             .header("Authorization", format!("Bearer {}", openai_key))
//             .header("OpenAI-Beta", "assistants=v2")
//             .json(&json_payload)
//             .send()
//             .await?;

//         let response_text = response.text().await?;

//         log::info!("Received response from send_message_to_thread: {}", response_text);
//         log::info!("Step 3 complete");

//         let response_json: serde_json::Value = serde_json::from_str(&response_text)?;

//         // Now fetch the assistant's response
//         match get_last_assistant_message(openai_key, thread_id).await {
//             Ok(assistant_response) => return Ok(assistant_response),
//             Err(e) => log::error!("Error getting last assistant message: {:?}", e),
//         };
//     }

//     // If all retries failed, return error
//     Err(anyhow::anyhow!("Failed to send message after multiple attempts due to active run"))
// }

pub async fn first_loop(openai_key: &str, thread_id: &str, assistant_id: &str) -> anyhow::Result<String> {
    log::info!("got to first_loop");
    // log::info!("Step 2 should be starting soon.");
    // log::info!("Since I am already adding the message to the json_payload in step 2,");
    // log::info!("Step 3 is completed when the response from step 2 is received");
    // let thread_id = match create_openai_thread(&openai_key, message).await {
    //     Ok(thread_id) => thread_id,
    //     Err(e) => {
    //         log::error!("Failed to create thread: {}", e);
    //         let no_makey = "could not makey thread";
    //         no_makey.to_string()
    //     }
    // };
//REMOVED because for the first loop, since the initial message is already sent when the\
//      thread is created, there's no point in sending the message again. so step 3 is done
//       when 2 is done
    // let client = reqwest::Client::new();

    // let json_payload = serde_json::json!({
    //     "role": "user",
    //     "content": message
    // });

    // log::info!("Step 3 initializing: aka add a user's message to the thread");
    // log::info!("aka POST https://api.openai.com/v1/threads/{thread_id}/messages");

    // let response = client.post(&format!("https://api.openai.com/v1/threads/{}/messages", thread_id))
    // .header("Content-Type", "application/json")
    // .header("Authorization", format!("Bearer {}", openai_key))
    // .header("OpenAI-Beta", "assistants=v2")
    // .json(&json_payload)
    // .send()
    // .await?;

    // let response_text = response.text().await?;
    // log::info!("Step 3 complete");
    // log::info!("Received response from add a user's message to the thread: {}", response_text);

    log::info!("Step 4 initializing. aka Run the assistant");
    log::info!("aka POST https://api.openai.com/v1/threads/{thread_id}/runs");

    let run_id = match create_run_on_thread(&openai_key, &thread_id, &assistant_id).await {
        Ok(run_id) => run_id,
        Err(e) => {
            log::error!("Failed to create run: {}", e);
            let no_run = "couldn't make run";
            no_run.to_string()
        }
    };

    const MAX_RETRIES: u32 = 10;
    for attempt in 0..MAX_RETRIES {
        // Check if the run is active
        log::info!("Step 5 shoudl be starting soon");
        if is_run_active(openai_key, &thread_id, &run_id).await? {
            log::warn!("Active run detected, retrying... Attempt {}", attempt + 1);
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            //continue to next iteration of for loop
            continue;
        }
        else {
            //break out of of for loop
            break;
        }
    }
    log::info!("Step 6 should be starting soon");
    match get_last_assistant_message(openai_key, &thread_id).await {
        Ok(response) => {
            //log::info!("The last message from the assistant is: {}", response);
            Ok(response)
        },
        Err(e) => {
            log::error!("Failed to get the last assistant message: {}", e);
            Ok("Failed to retrieve the assistant's response. Please try again later.".to_string())
        }
    }

    // let we_did_it = "Success";
    // Ok(we_did_it.to_string())
}


pub async fn second_message_and_so_on(openai_key: &str, thread_id: &str, text: &str, assistant_id: &str) -> anyhow::Result<String> {
    //step 3
        log::info!("since step 2 is already done, aka create the thread, we'll move on to step 3.");
        let client = reqwest::Client::new();

        let json_payload = serde_json::json!({
            "role": "user",
            "content": text
        });

        log::info!("Step 3 initializing: aka add a user's message to the thread");
        log::info!("aka POST https://api.openai.com/v1/threads/{thread_id}/messages");

        let response = client.post(&format!("https://api.openai.com/v1/threads/{}/messages", thread_id))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", openai_key))
        .header("OpenAI-Beta", "assistants=v2")
        .json(&json_payload)
        .send()
        .await?;

        let response_text = response.text().await?;
        log::info!("Step 3 complete");
        log::info!("Received response from add a user's message to the thread: {}", response_text);

    //step 4
        log::info!("Step 4 initializing. aka Run the assistant");
        log::info!("aka POST https://api.openai.com/v1/threads/{thread_id}/runs");

        let run_id = match create_run_on_thread(&openai_key, &thread_id, &assistant_id).await {
            Ok(run_id) => run_id,
            Err(e) => {
                log::error!("Failed to create run: {}", e);
                let no_run = "couldn't make run";
                no_run.to_string()
            }
        };
    //step 5
        const MAX_RETRIES: u32 = 10;
        for attempt in 0..MAX_RETRIES {
            // Check if the run is active
            log::info!("Step 5 shoudl be starting soon");
            if is_run_active(openai_key, &thread_id, &run_id).await? {
                log::warn!("Active run detected, retrying... Attempt {}", attempt + 1);
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                //continue to next iteration of for loop
                continue;
            }
            else {
                //break out of of for loop
                break;
            }
        }
    
    //step 6
        log::info!("Step 6 should be starting soon");
        match get_last_assistant_message(openai_key, &thread_id).await {
            Ok(response) => {
                //log::info!("The last message from the assistant is: {}", response);
                Ok(response)
            },
            Err(e) => {
                log::error!("Failed to get the last assistant message: {}", e);
                Ok("Failed to retrieve the assistant's response. Please try again later.".to_string())
            }
        }
}


pub async fn send_next_message(thread_id: &str, text: &str) -> anyhow::Result<()> {
    let client = Client::new();

    log::info!("message we're about to send: {}\n", text);
    log::info!("Step 3 initiating. AKA: Add a user's message to the thread");
    log::info!("AKA: POST https://api.openai.com/v1/threads/{thread_id}/messages");

    let response = client.post(&format!("https://api.openai.com/v1/threads/{}/messages", thread_id))
        .header("Content-Type", "application/json")
        .header("Authorization", "Bearer YOUR_OPEN_AI_KEY")
        .body(serde_json::json!({
            "role": "user",
            "content": text
        }).to_string())
        .send()
        .await?;
    let response_text = response.text().await?;
    
    log::info!("Step 3 complete");
    log::info!("response from POST https://api.openai.com/v1/threads/{thread_id}/messages:
    {}", response_text);

    Ok(())
}

