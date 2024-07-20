// src/main.rs

use tokio::join;
use dotenv::dotenv;
use std::env;
use webhooks_server::webhooks::run_webhook_server;
use webhooks_server::telegram::run_telegram_bot;



use std::fs;
//use std::io::Write;
use log::info;
use log4rs::{
    append::file::FileAppender, config::{Appender, Config, Root}, encode::pattern::PatternEncoder,
};
#[tokio::main]
async fn main() {
    dotenv().ok(); // Load environment variables from .env file
    
    let log_file_path = env::var("LOG_FILE_PATH").unwrap_or_else(|_| "logs/webhooks_server.log".to_string());

    // Ensure the logs directory exists
    fs::create_dir_all("logs").expect("Failed to create logs directory");

    // Configure file logging
    let logfile = log4rs::append::file::FileAppender::builder()
        .encoder(Box::new(log4rs::encode::pattern::PatternEncoder::new("{d} - {l} - {m}{n}")))
        .build(log_file_path)
        .unwrap();

    // Build the logger configuration
    let config = log4rs::config::Config::builder()
        .appender(log4rs::config::Appender::builder().build("logfile", Box::new(logfile)))
        .build(log4rs::config::Root::builder().appender("logfile").build(log::LevelFilter::Info))
        .unwrap();

    // Initialize logger
    log4rs::init_config(config).unwrap();

    log::info!("Logging started");

    // Ensure environment variables are set
    let _openai_key = env::var("OPENAI_KEY").expect("OPENAI_KEY not set");
    let _teloxide_token = env::var("TELOXIDE_TOKEN").expect("TELOXIDE_TOKEN not set");

    // Create database config
    let mut cfg = deadpool_postgres::Config::new();
    cfg.host = Some(env::var("TELEGRAM_DATABASE_HOST").expect("TELEGRAM_DATABASE_HOST not set"));
    cfg.port = Some(env::var("TELEGRAM_DATABASE_PORT").expect("TELEGRAM_DATABASE_PORT not set").parse().expect("Invalid port"));
    cfg.user = Some(env::var("TELEGRAM_DATABASE_USER").expect("TELEGRAM_DATABASE_USER not set"));
    cfg.password = Some(env::var("TELEGRAM_DATABASE_PASSWORD").expect("TELEGRAM_DATABASE_PASSWORD not set"));
    cfg.dbname = Some(env::var("TELEGRAM_DATABASE_NAME").expect("TELEGRAM_DATABASE_NAME not set"));
    cfg.manager = Some(deadpool_postgres::ManagerConfig { recycling_method: deadpool_postgres::RecyclingMethod::Fast });

    // Create connection pool
    let pool = cfg.create_pool(None, tokio_postgres::NoTls).unwrap();
    
    log::info!("Database connection pool created");

    // Pass the pool to the webhook server and telegram bot
    let webhook_server = {
        let pool = pool.clone();
        tokio::spawn(async move {
            run_webhook_server(pool).await;
            log::info!("Webhook server started");
        })
    };

    let telegram_bot = {
        let pool = pool.clone();
        tokio::spawn(async move {
            run_telegram_bot(pool).await;
            log::info!("Telegram bot started");
        })
    };

    let _ = tokio::join!(webhook_server, telegram_bot);
}