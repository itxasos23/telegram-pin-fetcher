//! Example to print the ID and title of all the dialogs.
//!
//! The `TG_ID` and `TG_HASH` environment variables must be set (learn how to do it for
//! [Windows](https://ss64.com/nt/set.html) or [Linux](https://ss64.com/bash/export.html))
//! to Telegram's API ID and API hash respectively.
//!
//! Then, run it as:
//!
//! ```sh
//! cargo run --example dialogs
//! ```

use grammers_client::{Client, Config, SignInError};
use grammers_session::Session;
use grammers_tl_types as tl;
use log;
use simple_logger::SimpleLogger;
use std::io::{self, BufRead as _, Write as _};
use std::fs;
use tokio::runtime;
use serde_derive::{Deserialize, Serialize};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const SESSION_FILE: &str = "dialogs.session";

#[derive(Deserialize)]
struct FileConfig {
    telegram_api_creds: CredsConfig,
    config: UsersConfig
}

#[derive(Deserialize)]
struct UsersConfig {
    usernames: Vec<String>
}

#[derive(Deserialize)]
struct CredsConfig {
    api_id: i32,
    api_hash: String
}

#[derive(Serialize, Debug)]
struct Message {
    sender: String,
    text: String,
    date: String 
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

async fn async_main() -> Result<()> {
    SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .init()
        .unwrap();

    let config_file_contents = fs::read_to_string("config.toml").unwrap();
    let creds_toml: FileConfig = toml::from_str(&config_file_contents).unwrap();

    let api_id = creds_toml.telegram_api_creds.api_id;
    let api_hash = creds_toml.telegram_api_creds.api_hash;

    println!("Connecting to Telegram...");
    let client = Client::connect(Config {
        session: Session::load_file_or_create(SESSION_FILE)?,
        api_id,
        api_hash: api_hash.clone(),
        params: Default::default(),
    })
    .await?;
    println!("Connected!");

    // If we can't save the session, sign out once we're done.
    let mut sign_out = false;

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
        match client.session().save_to_file(SESSION_FILE) {
            Ok(_) => {}
            Err(e) => {
                println!(
                    "NOTE: failed to save the session, will sign out when done: {}",
                    e
                );
                sign_out = true;
            }
        }
    }

    // while let Some(dialog) = dialogs.next().await? {let chat = dialog.chat(); println!("- {: >10} {}", chat.id(), chat.name());}
    
    let chat_names = creds_toml.config.usernames;
    dbg!(&chat_names);

    let mut messages = Vec::<Message>::new();

    for chat_name in chat_names {

        let maybe_chat = client.resolve_username(chat_name.as_str()).await?;
        let chat = maybe_chat.unwrap_or_else(|| panic!("Chat {} could not be found", chat_name));
        let mut pinned_messages = client.search_messages(&chat).filter(tl::enums::MessagesFilter::InputMessagesFilterPinned);

        println!(
            "Chat {} has {} total pinned messages.",
            chat_name,
            pinned_messages.total().await.unwrap()
        );

        while let Some(msg) = pinned_messages.next().await? {
            if let Some(_) = msg.media() {continue}
            let sender = msg.sender().unwrap();
            let text = msg.text();
            let date = msg.date().date_naive();

            messages.push(Message {
                sender: sender.username().unwrap().to_string(), 
                text: text.to_string(), 
                date: date.to_string()
            });

        }
    }

    messages.sort_by(| a, b| a.date.cmp(&b.date));

    let mut file = fs::File::create("out.json")?;
    file.write_all(serde_json::to_string(&messages)?.as_bytes())?;

    if sign_out {
        // TODO revisit examples and get rid of "handle references" (also, this panics)
        drop(client.sign_out_disconnect().await);
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