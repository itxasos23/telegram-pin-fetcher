use chrono;
use grammers_client::{Client, Config, SignInError};
use grammers_session::Session;
use grammers_tl_types as tl;
use home;
use log;
use serde_derive::{Deserialize, Serialize};
use simple_logger::SimpleLogger;
use std::fs;
use std::io::{self, BufRead as _, Write as _};
use std::path::PathBuf;
use tokio::runtime;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Deserialize)]
struct FileConfig {
    telegram_api_creds: CredsConfig,
    config: UsersConfig,
    upload: UploadConfig,
}

#[derive(Deserialize)]
struct UsersConfig {
    usernames: Vec<String>,
}

#[derive(Deserialize)]
struct CredsConfig {
    api_id: i32,
    api_hash: String,
}

#[derive(Deserialize)]
struct UploadConfig {
    provider: String,
    api_token: String,
}

#[derive(Serialize, Debug)]
struct Message {
    sender: String,
    text: String,
    date: String,
}

fn prompt(message: &str) -> Result<String> {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    stdout.write_all(message.as_bytes())?;
    stdout.flush()?;

    let stdin = io::stdin();
    let mut stdin = stdin.lock();

    let mut line = String::new();
    stdin.read_line(&mut line)?;
    Ok(line)
}

async fn get_pinned_messages(client: Client, creds_toml: &FileConfig) -> Result<Vec<Message>> {
    let chat_names = &creds_toml.config.usernames;
    let mut messages = Vec::<Message>::new();

    for chat_name in chat_names {
        let maybe_chat = client.resolve_username(chat_name.as_str()).await?;
        let chat = maybe_chat.unwrap_or_else(|| panic!("Chat {} could not be found", chat_name));
        let mut pinned_messages = client
            .search_messages(&chat)
            .filter(tl::enums::MessagesFilter::InputMessagesFilterPinned);

        println!(
            "Chat {} has {} total pinned messages.",
            chat_name,
            pinned_messages.total().await.unwrap()
        );

        while let Some(msg) = pinned_messages.next().await? {
            if let Some(_) = msg.media() {
                continue;
            }
            let sender = msg.sender().unwrap();
            let text = msg.text();
            let date = msg.date().date_naive();

            messages.push(Message {
                sender: sender.username().unwrap().to_string(),
                text: text.to_string(),
                date: date.to_string(),
            });
        }
    }

    messages.sort_by(|a, b| a.date.cmp(&b.date));

    Ok(messages)
}

async fn login_and_get_pinned_messages(
    config: &FileConfig,
    session_file: &PathBuf,
) -> Result<Vec<Message>> {
    let client = Client::connect(Config {
        session: Session::load_file_or_create(&session_file).unwrap(),
        api_id: config.telegram_api_creds.api_id.clone(),
        api_hash: config.telegram_api_creds.api_hash.clone(),
        params: Default::default(),
    })
    .await?;

    if !client.is_authorized().await? {
        println!("Signing in...");
        let phone = prompt("Enter your phone number (international format): ")?;
        let token = client.request_login_code(&phone).await?;
        let code = prompt("Enter the code you received: ")?;
        let signed_in = client.sign_in(&token, &code).await;
        match signed_in {
            Err(SignInError::PasswordRequired(password_token)) => {
                // Note: this `prompt` method will echo the password in the console.
                //       Real code might want to use a better way to handle this.
                let hint = password_token.hint().unwrap_or("None");
                let prompt_message = format!("Enter the password (hint {}): ", &hint);
                let password = prompt(prompt_message.as_str())?;

                client
                    .check_password(password_token, password.trim())
                    .await?;
            }
            Ok(_) => (),
            Err(e) => panic!("{}", e),
        };
        println!("Signed in!");
    }

    Ok(get_pinned_messages(client, config).await.unwrap())
}

fn get_config_dirs() -> (PathBuf, PathBuf) {
    let mut config_dir = match home::home_dir() {
        Some(path) => path,
        None => panic!("Could not find home dir"),
    };

    config_dir.push(".config");
    config_dir.push("telegram_pinned");

    let mut config_file = config_dir.clone();
    config_file.push("config.toml");

    let mut session_file = config_dir.clone();
    session_file.push("telegram.session");

    (config_file, session_file)
}

async fn async_main() -> Result<()> {
    SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .init()
        .unwrap();

    let (config_file_path, session_file_path) = get_config_dirs();

    let config_file_contents = fs::read_to_string(&config_file_path).unwrap();
    let creds_toml: FileConfig = toml::from_str(&config_file_contents).unwrap();

    let messages = login_and_get_pinned_messages(&creds_toml, &session_file_path).await?;

    match upload_messages(&creds_toml, messages).await {
        Err(_) => println!("Error uploading messages"),
        _ => (),
    };

    Ok(())
}

async fn upload_messages(creds_toml: &FileConfig, messages: Vec<Message>) -> Result<()> {
    let payload = serde_json::to_string(&messages).unwrap().clone();

    if creds_toml.upload.provider != "gofile" {
        panic!("Only gofile upload provider is supported.");
    }

    let http_client = reqwest::Client::new();
    let payload_bytes = String::from_utf8(payload.into_bytes()).unwrap();
    let mut file_part_headers = reqwest::header::HeaderMap::new();
    file_part_headers.insert(
        reqwest::header::CONTENT_TYPE,
        "application/json".parse().unwrap(),
    );

    let now = chrono::offset::Utc::now();
    let date = now.date_naive();
    let filename = date.format("%Y-%m-%d.json").to_string();

    let file_part = reqwest::multipart::Part::bytes(payload_bytes.into_bytes())
        .file_name(filename)
        .headers(file_part_headers);

    let form = reqwest::multipart::Form::new()
        .part("file", file_part)
        .text("folderId", "cf71f5f5-d849-4c80-94c7-eb73e5253c86");

    let req = http_client
        .post("https://store1.gofile.io/contents/uploadfile")
        .bearer_auth(&creds_toml.upload.api_token)
        .multipart(form);

    match req.send().await {
        Ok(res) => println!("Response from remote: {}", res.text().await?),
        Err(e) => println!("Error pushing data to remote: {}", e)
    }

    Ok(())
}

fn main() -> Result<()> {
    runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async_main())
}
