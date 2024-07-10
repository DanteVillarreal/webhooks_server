// src/database.rs

pub async fn insert_user(pool: deadpool_postgres::Pool, user: crate::DBUser) -> Result<(), anyhow::Error> {
    // Get a client from the pool, handling the pool error explicitly
    let client = pool.get().await.map_err(|e| {
        log::error!("Failed to get client from pool: {:?}", e);
        anyhow::Error::new(e)
    })?;
    client.execute(
        "INSERT INTO users (user_id, first_name, last_name, username) VALUES ($1, $2, $3, $4)
         ON CONFLICT (user_id) DO UPDATE SET first_name = EXCLUDED.first_name, last_name = EXCLUDED.last_name, username = EXCLUDED.username",
        &[&user.id, &user.first_name, &user.last_name, &user.username]
    ).await?;
    Ok(())
}

pub async fn insert_thread(pool: deadpool_postgres::Pool, thread_id: i32, user_id: i64, openai_thread_id: &str) -> Result<(), anyhow::Error> {
    // Get a client from the pool, handling the pool error explicitly
    let client = pool.get().await.map_err(|e| {
        log::error!("Failed to get client from pool: {:?}", e);
        anyhow::Error::new(e)
    })?;
    
    client.execute(
        "INSERT INTO threads (thread_id, user_id, openai_thread_id) VALUES ($1, $2, $3)
         ON CONFLICT (thread_id) DO UPDATE SET user_id = EXCLUDED.user_id, openai_thread_id = EXCLUDED.openai_thread_id",
        &[&thread_id, &user_id, &openai_thread_id]
    ).await?;

    Ok(())
}

pub async fn insert_message(pool: deadpool_postgres::Pool, thread_id: i32, sender: &str, content: &str, message_type: &str) -> Result<(), anyhow::Error> {
    // Get a client from the pool, handling the pool error explicitly
    let client = pool.get().await.map_err(|e| {
        log::error!("Failed to get client from pool: {:?}", e);
        anyhow::Error::new(e)
    })?;
    
    client.execute(
        "INSERT INTO messages (thread_id, sender, content, message_type) VALUES ($1, $2, $3, $4)",
        &[&thread_id, &sender, &content, &message_type]
    ).await?;

    Ok(())
}