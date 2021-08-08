use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Deserializer};
use serde_json::Value;
use winit::event_loop::EventLoopProxy;

use crate::Events;

/// Default config filename
pub const DEFAULT_CONFIG_FILE: &'static str = "config";
pub const UPDATE_CHANNELS_TIME: Duration = Duration::from_secs(60);
pub const READ_CONFIG_FILE_TIME: Duration = Duration::from_secs(3);

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct Channel {
    pub name: String,
    pub is_online: bool,
    pub title: Option<String>,
    pub viewers: Option<u64>,
}

// When we read the channels, we only have the name,
//  so we just read the name and fill the other fields.
impl<'a> Deserialize<'a> for Channel {
    fn deserialize<D>(deserializer: D) -> Result<Channel, D::Error>
    where
        D: Deserializer<'a>,
    {
        let value: serde_json::Value = serde::Deserialize::deserialize(deserializer)?;

        let name = value
            .as_str()
            .ok_or(serde::de::Error::custom("expected a string"))?;

        Ok(Channel {
            name: String::from(name),
            is_online: false,
            title: None,
            viewers: None,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct State {
    pub client: String,
    pub secret: String,
    pub channels: Vec<Channel>,
}

fn state_migrate(config: &Arc<Mutex<State>>, new_config: State) {
    let mut local_config = config.lock().unwrap();

    local_config.client = new_config.client.clone();
    local_config.secret = new_config.secret.clone();

    // We first save a copy of the old channels, then we set the state to the new one.
    // With the old channels, the channels that exist in the new one just have to be updated.
    // And with this, we can save the previous state which will be refreshed the next time that the data is retrieved.
    let old_channels = local_config.channels.clone();

    local_config.channels = new_config.channels;

    for channel in &mut local_config.channels {
        for old_channel in &old_channels {
            if channel.name == old_channel.name {
                // Save the old data.
                *channel = old_channel.clone();
            }
        }
    }
}

pub fn read_config() -> State {
    let file = std::fs::File::open(DEFAULT_CONFIG_FILE)
        .expect("please ensure that there's a valid secret file in the same directory.");
    let reader = std::io::BufReader::new(file);

    return serde_json::from_reader(reader).expect("valid config format.");
}

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
                    c.is_online = true;
                    c.title = title.as_str().map(|x| String::from(x).trim().to_string());
                    c.viewers = viewers.as_u64();
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

fn compare_state(a: &State, b: &State) -> bool {
    if a.client != b.client || b.secret != b.secret {
        return false;
    }

    if a.channels.len() != b.channels.len() {
        return false;
    }

    a.channels
        .iter()
        .zip(b.channels.iter())
        .filter(|(a, b)| *a.name != *b.name)
        .count()
        == 0
}

/// Periodically refresh the global state from the configuration file.
pub async fn refresh_config(config: Arc<Mutex<State>>, proxy: &EventLoopProxy<Events>) {
    loop {
        let old_config = {
            // Copy the config so we can compare it.
            config.lock().unwrap().clone()
        };

        let new_config = read_config();

        if !compare_state(&old_config, &new_config) {
            state_migrate(&config, new_config);

            proxy.send_event(Events::UpdatedChannels).ok();
        }

        std::thread::sleep(READ_CONFIG_FILE_TIME);
    }
}
