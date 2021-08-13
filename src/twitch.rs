use crate::config;
use crate::config::State;
use crate::send_notification;
use crate::Events;

use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::Value;

use winit::event_loop::EventLoopProxy;

pub const UPDATE_CHANNELS_TIME: Duration = Duration::from_secs(60);
pub const READ_CONFIG_FILE_TIME: Duration = Duration::from_secs(3);

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
        .expect("valid response.")
        .json::<Value>()
        .await
        .expect("valid JSON message.");

    if !response.is_object() {
        panic!("invalid response: not an object.")
    }

    if !response["access_token"].is_string() {
        if response["message"].is_string() {
            panic!("invalid credentials.")
        }

        panic!("invalid response: doesn't have the field 'access_token'.")
    }

    let token = response["access_token"]
        .as_str()
        .expect("valid access token.");

    return format!("Bearer {}", token);
}

async fn update_channels(client: &reqwest::Client, token: &String, config: &Arc<Mutex<State>>) {
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
        .await
        .expect("valid response.")
        .json::<Value>()
        .await
        .expect("valid JSON message.");

    let contents = response
        .as_object()
        .expect("unknown response: not an object.");

    if contents.contains_key("error") || !contents.contains_key("data") {
        panic!("Invalid API response received! Please check if the channels are valid.");
    }

    let data = contents["data"].as_array().expect("invalid data.");

    for channel in data {
        let title = &channel["title"];
        let name = &channel["user_name"];
        let viewers = &channel["viewer_count"];

        if !name.is_string() {
            continue;
        }
        let name = name.as_str().expect("expected to get an username.");

        {
            let mut local_config = config.lock().unwrap();

            for c in &mut local_config.channels {
                if c.name == name {
                    c.title = title.as_str().map(|x| String::from(x).trim().to_string());
                    c.viewers = viewers.as_u64();

                    // If the channel wasn't live before, notify the user.
                    if !c.is_online {
                        let notification_title = title.as_str().unwrap_or(name);
                        let notification_text = format!(
                            "{} is live! ({} viewers)",
                            name,
                            viewers
                                .as_u64()
                                .map(|x| x.to_string())
                                .unwrap_or_else(|| String::from("unknown"))
                        );

                        send_notification(&notification_title, &notification_text);
                    }

                    c.is_online = true;
                }
            }
        }
    }
}

pub async fn listen_for_events(config: Arc<Mutex<State>>, proxy: &EventLoopProxy<Events>) {
    let client = reqwest::Client::new();

    let token = get_token(&client, &config).await;

    loop {
        update_channels(&client, &token, &config).await;

        proxy.send_event(Events::UpdatedChannels).ok();

        std::thread::sleep(UPDATE_CHANNELS_TIME);
    }
}

/// Periodically refresh the global state from the configuration file.
pub async fn refresh_config(config: Arc<Mutex<State>>, proxy: &EventLoopProxy<Events>) {
    loop {
        let old_config = {
            // Copy the config so we can compare it.
            config.lock().unwrap().clone()
        };

        let new_config = config::read();

        if !config::compare(&old_config, &new_config) {
            config::migrate(&config, new_config);

            proxy.send_event(Events::UpdatedChannels).ok();
        }

        std::thread::sleep(READ_CONFIG_FILE_TIME);
    }
}
