// src/lib.rs

use serde::{Deserialize, Serialize};
use reqwest::Client;
use std::env;
use uuid::Uuid;
use tokio::fs::File;
//use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use anyhow;
//use reqwest::multipart;
use anyhow::Context;
use anyhow::Result;
pub mod webhooks;
pub mod telegram;
pub mod database;
use serde_json::Value;
use rand::SeedableRng;


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
    pub voice: Option<Voice>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Audio {
    pub file_id: String,
    pub file_unique_id: String,
    pub duration: u64,
    pub file_size: Option<u64>,
    pub file_path: Option<String>,
    pub mime_type: Option<String>,
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

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Voice {
    pub file_id: String,
    pub file_unique_id: String,
    pub duration: u64,
    pub mime_type: Option<String>,
    pub file_size: Option<u64>,
    pub file_path: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DBUser {
    pub id: i64,  // Change to i64
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub username: Option<String>,
}









pub async fn handle_message_handler(message: Message, openai_key: String,) {
    log::info!("Audio: step 0: Got to handle message handler fn");
    match handle_message(message.clone(), openai_key.clone()).await {
        Ok(_) => (),
        Err(e) => log::error!("Error handling message: {:?}", e),
    }
}

pub async fn handle_message(message: Message, openai_key: String, ) -> Result<(), anyhow::Error> {
    log::info!("Audio: step 1: god to handle_message fn");
    let bot_token = env::var("TELOXIDE_TOKEN")
        .expect("TELOXIDE_TOKEN does not exist. check naming");
    let chat_id = message.chat.id;

    if let Some(ref text) = message.text {
        log::info!("about to handle message as a text");
        handle_text_message(&bot_token, &chat_id, text, &openai_key).await?;
    } else if let Some(ref audio) = message.audio {
        log::info!("about to handle message as audio");
        handle_audio_message(&bot_token, audio, &openai_key).await?;
    } else if let Some(ref voice) = message.voice {
        log::info!("about to handle message as voice");
        handle_voice_message(&bot_token, voice, &openai_key).await?;
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

async fn handle_audio_message(bot_token: &str, audio: &Audio, openai_key: &str) -> Result<String, anyhow::Error> {
    log::info!("Audio: step 2: In handle_audio_message fn");

    // Get the file path from Telegram using the get_file function
    let file_path_on_telegram = get_file(bot_token, &audio.file_id).await?;
    log::info!("Audio: step 2: In handle_audio_message. got file path");
    log::info!("Audio: step 2: In handle_audio_message. File_path is {file_path_on_telegram}");

    // Download the audio file from Telegram
    let file_url = format!("https://api.telegram.org/file/bot{}/{}", bot_token, &file_path_on_telegram);
    log::info!("Audio: step 3: about to download audio file");
    log::info!("file_url is {file_url}");
    let file_name = download_file(&file_url, &audio.file_id, audio.mime_type.as_deref()).await?;

    // Call OpenAI API to transcribe audio
    log::info!("Audio: step 4: about to transcribe the audio message");
    let transcription = transcribe_audio(openai_key, &file_name, audio.mime_type.as_deref()).await?;
    log::info!("audio message transcribed to: {}", transcription);

    // Return the transcription instead of sending it to Telegram
    Ok(transcription)
}
async fn handle_voice_message(bot_token: &str, voice: &Voice, openai_key: &str) -> Result<String, anyhow::Error> {
    log::info!("Voice: step 2: In handle_voice_message fn");

    // Get the file path from Telegram using the get_file function
    let file_path_on_telegram = get_file(bot_token, &voice.file_id).await?;
    log::info!("Voice: step 2: In handle_voice_message. got file path: {}", file_path_on_telegram);

    // Download the voice file from Telegram
    let file_url = format!("https://api.telegram.org/file/bot{}/{}", bot_token, file_path_on_telegram);
    log::info!("Voice: step 3: about to download voice file");
    let file_name = download_file(&file_url, &voice.file_id, voice.mime_type.as_deref()).await?;

    // Call OpenAI API to transcribe voice
    log::info!("Voice: step 4: about to transcribe the voice message");
    let transcription = transcribe_audio(openai_key, &file_name, voice.mime_type.as_deref()).await?;
    log::info!("voice message transcribed to: {}", transcription);

    Ok(transcription)
}
// async fn handle_audio_message(bot_token: &str, chat_id: &u64, audio: &Audio, openai_key: &str) -> Result<(), anyhow::Error> {
//     log::info!("Audio: step 2: In handle_audio_message fn");

//     // Get the file path from Telegram using the get_file function
//     let file_path_on_telegram = get_file(bot_token, &audio.file_id).await?;
//     log::info!("Audio: step 2: In handle_audio_message. got file path");
//     log::info!("Audio: step 2: In handle_audio_message. File_path is {file_path_on_telegram}");

//     // Download the audio file from Telegram
//     let file_url = format!("https://api.telegram.org/file/bot{}/{}", bot_token, &file_path_on_telegram);
//     log::info!("Audio: step 3: about to download audio file");
//     log::info!("file_url is {file_url}");
//     let file_name = download_file(&file_url, &audio.file_id, audio.mime_type.as_deref()).await?;

//     // Call OpenAI API to transcribe audio
//     log::info!("Audio: step 4: about to transcribe the audio message");
//     let transcription = transcribe_audio(openai_key, &file_name, audio.mime_type.as_deref()).await?;
//     log::info!("audio message transcribed to: {}", transcription);

//     let bot = Client::new();
//     bot.post(&format!("https://api.telegram.org/bot{}/sendMessage", bot_token))
//         .json(&serde_json::json!({
//             "chat_id": chat_id,
//             "text": transcription,
//         }))
//         .send()
//         .await?;

//     Ok(())
// }



async fn get_file(bot_token: &str, file_id: &str) -> Result<String> {
    log::info!("Audio: step 2 initializing. in get_file right now");
    let client = Client::new();
    let res: Value = client.post(&format!("https://api.telegram.org/bot{}/getFile", bot_token))
        .form(&[("file_id", file_id)])
        .send()
        .await?
        .json()
        .await?;
    let file_path = res["result"]["file_path"].as_str().unwrap().to_string();
    Ok(file_path)
}


async fn download_file(url: &str, file_id: &str, mime_type: Option<&str>) -> Result<String, anyhow::Error> {
    log::info!("Audio: step 3: in download_file fn");
    
    let client = Client::new();
    log::info!("Audio: step 3 initializing");
    
    // Send POST request to the URL to GET the file_path
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("Failed to send GET request to URL: {}", url))?;
    
    // Ensure the request was successful
    if !response.status().is_success() {
        let error_message = format!("Received non-200 status code ({}) when trying to access URL: {}", response.status(), url);
        log::error!("{}", error_message);
        anyhow::bail!(error_message);
    }
    log::info!("Audio: step 3: in download_file: mime type is {:?}", mime_type);
    // Determine the file extension based on the MIME type
    let file_extension = match mime_type {
        Some("audio/flac") => "flac",
        Some("audio/m4a") => "m4a",
        Some("audio/mp3") => "mp3",
        Some("audio/mp4") => "mp4",
        Some("audio/mpeg") => "mp3",
        Some("audio/mpga") => "mpga",
        Some("audio/oga") => "oga",
        Some("audio/webm") => "webm",
        Some("audio/wav") => "wav",
        Some("audio/ogg") => "ogg",
        // Add more MIME types and their corresponding file extensions as needed
        _ => "unknown",
    };

    let filename = format!("{}.{}", Uuid::new_v4(), file_extension);
    log::info!("Audio: step 3: in download_file. filename is {filename}");
    
    // Create the file
    let mut file = File::create(&filename)
        .await
        .with_context(|| format!("Failed to create file: {}", filename))?;
    
    // Extract the response content
    let content = response
        .bytes()
        .await
        .with_context(|| "Failed to read content from response".to_string())?;

    log::info!("Size of content to be written: {}", content.len());
    
    // Write content to the file
    file.write_all(&content)
        .await
        .with_context(|| format!("Failed to write content to file: {}", filename))?;

    // Ensure all intermediately buffered contents reach their destination.
    file.flush()
        .await
        .with_context(|| format!("Failed to flush content to file: {}", filename))?;

    // Get the size of the file after writing
    let metadata = tokio::fs::metadata(&filename).await?;
    log::info!("Size of file after writing: {}", metadata.len());
    
    if mime_type.unwrap() == "audio/m4a" {
        // Try to read the metadata with mp4ameta
        let tag = match mp4ameta::Tag::read_from_path(&filename) {
            Ok(tag) => tag,
            Err(e) => {
                let error_message = format!("File is corrupt: {}. Error: {:?}", filename, e);
                log::error!("{}", error_message);
                anyhow::bail!(error_message);
            }
        };
   

        log::info!("Audio: step 3: in download_file: Audio file metadata: {:?}", tag);
        log::info!("Audio: step 3: if it got here, file is not corrupt");
    }
    log::info!("Audio: step 3 completed successfully");
    Ok(filename)
}





// use rs_openai::audio::Audio;
async fn transcribe_audio(openai_key: &str, file_name: &str, mime_type: Option<&str>) -> Result<String, anyhow::Error> {
    log::info!("Audio: step 4: in transcribe_audio.");
    
    // Initialize reqwest client
    let client = reqwest::Client::new();

    // Open file
    log::info!("Audio: step 4 initializing: opening file");
    let audio_bytes = tokio::fs::read(file_name).await?;

    // Create a multipart file part
    let file_part = reqwest::multipart::Part::stream(audio_bytes)
        .file_name(file_name.to_string())
        .mime_str(mime_type.expect("Couldn't give it a mime type"))?;

    // Create the multipart form
    let form = reqwest::multipart::Form::new()
        .text("model", "whisper-1")
        .part("file", file_part);

    log::info!("Audio: step 4: sending request to OpenAI for transcription");

    // Send the POST request to the OpenAI API endpoint
    let response = client.post("https://api.openai.com/v1/audio/transcriptions")
        .header("Authorization", format!("Bearer {}", openai_key))
        .multipart(form)
        .send()
        .await
        .context("Failed to send the request to OpenAI")?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_else(|_| String::from("Failed to read response text"));
        anyhow::bail!("Received non-200 status code ({}) from OpenAI: {}", status, text);
    }

    // Parse the response body
    let response_json: serde_json::Value = response.json().await
        .context("Failed to parse the response from OpenAI")?;
    let transcription = response_json["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Transcription not found in response"))?
        .to_string();

    log::info!("audio message transcribed to: {}", transcription);

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

// Function to introduce a random delay between 10 and 20 seconds
async fn introduce_delay() {
    // Create an instance of StdRng from entropy using the fully qualified path
    let mut rng = rand::rngs::StdRng::from_entropy();
    
    // Use the fully qualified path for the gen_range method to specify that it belongs to the Rng trait
    let delay_seconds: u64 = rand::Rng::gen_range(&mut rng, 10..=20);
    
    // Use the fully qualified path for sleep and Duration functions
    tokio::time::sleep(tokio::time::Duration::from_secs(delay_seconds)).await;
}



//function to summarize:
use teloxide::Bot;
use teloxide::prelude::Requester;

use teloxide::types::ChatId;

pub async fn summarize_conversation(pool: &deadpool_postgres::Pool, message: &str, openai_key: &str, bot: &Bot, chat_id: i64
) -> anyhow::Result<()> {
    // Extract the username and assistant_id from the message
    let parts: Vec<&str> = message.split_whitespace().collect();
    let username = parts.get(1).unwrap_or(&"unknown");
    let assistant_id = parts.iter().find(|&&part| part.starts_with("asst_")).unwrap_or(&"unknown");

    let client = pool.get().await?;

    // Get user_id from username
    let query = "SELECT user_id FROM users WHERE username = $1";
    let user_id_row = client.query_one(query, &[&username]).await?;
    let user_id: i64 = user_id_row.get("user_id");

    // Get the thread_id for the given user_id and assistant_id
    let query = "SELECT thread_id FROM threads WHERE user_id = $1 AND assistant_id = $2";
    let thread_id_row = client.query_one(query, &[&user_id, &assistant_id]).await?;
    let thread_id: String = thread_id_row.get("thread_id");

    // Get messages for the thread_id
    let query = "SELECT sender, content FROM messages WHERE thread_id = $1 ORDER BY created_at ASC";
    let rows = client.query(query, &[&thread_id]).await?;

    let mut conversation = String::new();
    for row in rows {
        let sender: String = row.get("sender");
        let content: String = row.get("content");
        conversation.push_str(&format!("{}: {}\n", sender, content));
    }

    // Create a new initial message for the assistant to summarize the conversation
    let summarize_request = format!("{}", conversation);

    // Get or create a thread and log the user's request message
    let (new_thread_id, is_new_thread) = crate::telegram::get_or_create_thread(&pool, user_id, assistant_id, openai_key, &summarize_request).await?;

    if let Err(e) = crate::database::insert_message(pool.clone(), &new_thread_id, "user", &summarize_request, "text", assistant_id).await {
        log::error!("Failed to log user message: {:?}", e);
    }

    // Call first_loop to process the summarization request
    let response_result = if is_new_thread {
        first_loop(openai_key, &new_thread_id, assistant_id).await
    } else {
        second_message_and_so_on(openai_key, &new_thread_id, &summarize_request, assistant_id).await
    };

    match response_result {
        Ok(response_value) => {
            //introduce_delay().await;
            if let Err(e) = crate::database::insert_message(pool.clone(), &new_thread_id, "assistant", &response_value, "text", assistant_id).await {
                log::error!("Failed to log assistant message: {:?}", e);
            }
            bot.send_message(ChatId(chat_id), response_value).await?;
        },
        Err(e) => {
            log::error!("Failed to process summarization request: {:?}", e);
            bot.send_message(ChatId(chat_id), "Failed to summarize conversation. Please try again later.").await?;
        }
    }

    Ok(())
}
