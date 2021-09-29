use crate::config;
use crate::config::State;
use crate::send_notification;
use crate::Events;

use std::sync::mpsc;
use std::sync::mpsc::TryRecvError;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::Value;

use winit::event_loop::EventLoopProxy;

pub const UPDATE_CHANNELS_TIME: u64 = 60;
pub const READ_CONFIG_FILE_TIME: Duration = Duration::from_secs(3);
pub const MAX_RETRIES: u32 = 3;

async fn get_token(client: &reqwest::Client, config: &Arc<Mutex<State>>) -> String {
    // Get the mutex, build the URL based on the client & secret and unlock it.
    let url = {
        let local_config = config.lock().unwrap();

        format!(
            "https://id.twitch.tv/oauth2/token?client_id={}&client_secret={}&grant_type=client_credentials",
            local_config.client, local_config.secret
        )
    };

    let response: Value = client
        .post(url)
        .send()
        .await
        .expect("Valid response.")
        .json::<Value>()
        .await
        .expect("Valid JSON message.");

    if !response.is_object() {
        panic!("Invalid response: not an object.")
    }

    if !response["access_token"].is_string() {
        if response["message"].is_string() {
            panic!("Invalid credentials.")
        }

        panic!("Invalid response: doesn't have the field 'access_token'.")
    }

    let token = response["access_token"]
        .as_str()
        .expect("Valid access token.");

    return format!("Bearer {}", token);
}

async fn update_channels(
    client: &reqwest::Client,
    token: &String,
    config: &Arc<Mutex<State>>,
) -> Result<(), reqwest::Error> {
    let mut url = String::from("https://api.twitch.tv/helix/streams?");

    let client_id = {
        let local_config = config.lock().unwrap();

        for channel in &local_config.channels {
            url.push_str("user_login=");
            url.push_str(channel.name.as_str());
            url.push_str("&");
        }

        local_config.client.to_string()
    };

    let response = client
        .get(url)
        .header("Authorization", token)
        .header("Client-id", client_id)
        .send()
        .await?
        .json::<Value>()
        .await
        .expect("Valid JSON message.");

    let contents = response
        .as_object()
        .expect("Unknown response: not an object.");

    if contents.contains_key("error") || !contents.contains_key("data") {
        panic!("Invalid API response received! Please check if the channels are valid.");
    }

    let data = contents["data"].as_array().expect("Invalid data.");

    let local_config: &mut State = &mut config.lock().unwrap();

    for channel in &mut local_config.channels {
        // Is this channel present in the API response?
        let mut found: bool = false;

        for c in data {
            let name = &c["user_login"];
            let mut title = &c["title"];
            let viewers = &c["viewer_count"];

            if !name.is_string() || !viewers.is_u64() {
                continue;
            }

            // New accounts can stream without a title, but otherwise they are required to have one.
            let unknown_title = serde_json::Value::String(String::from("Unknown title"));
            if !title.is_string() {
                title = &unknown_title;
            }

            let name = name.as_str().expect("Expected to get an username.");
            let viewers = viewers.as_u64().expect("Expected to get the viewer count.");
            let title = title
                .as_str()
                .expect("Expected to get a title.")
                .trim()
                .to_string();

            // Check if we found the channel, not case sensitive.
            if channel.name.to_lowercase() == name.to_lowercase() {
                found = true;

                // If the title changed when the channel was live,
                //  we may want to notify the user based on their preferences.
                if channel.is_online
                    && channel.title != Some(title.clone())
                    && local_config.notify_title_changed.contains(&channel.name)
                {
                    let notification_text =
                        format!("{} has changed its title! ({} viewers)", name, viewers);

                    send_notification(&title, &notification_text);
                }

                // If the channel wasn't live before but is now, notify the user.
                if !channel.is_online {
                    let notification_text = format!("{} is live! ({} viewers)", name, viewers);

                    send_notification(&title, &notification_text);
                }

                channel.title = Some(title);
                channel.viewers = Some(viewers);
                channel.is_online = true;
            }
        }

        if !found {
            channel.is_online = false;
        }
    }

    Ok(())
}

pub async fn listen_for_events(
    config: Arc<Mutex<State>>,
    proxy: &EventLoopProxy<Events>,
    rx: mpsc::Receiver<()>,
) {
    let client = reqwest::Client::new();

    let token = get_token(&client, &config).await;

    // Sometimes a request might fail temporarily, we want to retry up to MAX_RETRIES times.
    let mut retry_counter = MAX_RETRIES;

    loop {
        match update_channels(&client, &token, &config).await {
            Ok(_) => {
                retry_counter = MAX_RETRIES;
            }
            Err(_) => {
                if retry_counter != 0 {
                    retry_counter -= 1;
                    std::thread::sleep(Duration::from_secs(1));
                    continue;
                }
            }
        };

        let last_update = std::time::SystemTime::now();

        proxy.send_event(Events::UpdatedChannels).ok();

        loop {
            std::thread::sleep(Duration::from_millis(500));

            match rx.try_recv() {
                Ok(_) => {
                    // Received a notification, the config must have changed, we have to update the channels.
                    break;
                }
                Err(TryRecvError::Empty) => {
                    // Has it been more than X seconds since the last update?
                    if let Some(time) = last_update.elapsed().ok() {
                        if time.as_secs() >= UPDATE_CHANNELS_TIME {
                            break; // If so, send the request to update the channels.
                        }
                    }
                }
                Err(TryRecvError::Disconnected) => {
                    panic!("The config/network channel was disconnected.")
                }
            }
        }
    }
}

/// Periodically refresh the global state from the configuration file.
pub async fn refresh_config(
    config: Arc<Mutex<State>>,
    proxy: &EventLoopProxy<Events>,
    update_tx: mpsc::Sender<()>,
) {
    loop {
        let old_config = {
            // Copy the config so we can compare it.
            config.lock().unwrap().clone()
        };

        let new_config = config::read();

        if old_config != new_config {
            config::migrate(&config, new_config);

            // Notify the network thread that we have to request an update.
            update_tx.send(()).ok();

            proxy.send_event(Events::UpdatedChannels).ok();
        }

        std::thread::sleep(READ_CONFIG_FILE_TIME);
    }
}
