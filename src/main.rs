mod config;

use config::Config;
use serde_json::Value;

// TODO: Better .expect() error messages.

async fn perform_auth(client: &reqwest::Client, config: &Config) -> String {
    let url = format!(
        "https://id.twitch.tv/oauth2/token?client_id={}&client_secret={}&grant_type=client_credentials",
        config.client, config.secret
    );

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

async fn update_channels(client: &reqwest::Client, token: String, config: &mut Config) {
    let mut url = String::from("https://api.twitch.tv/helix/streams?");

    // FIXME: This is slow. There's probably a better way of doing this.
    for channel in &config.channels {
        url.push_str("user_login=");
        url.push_str(channel.name.as_str());
        url.push_str("&");
    }

    let response = client
        .get(url)
        .header("Authorization", token)
        .header("Client-id", config.client.to_string())
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
        let name = name.as_str().unwrap();

        for c in &mut config.channels {
            if c.name == name {
                c.is_online = true;
                // FIXME: There's probably a better way of doing this.
                c.title = title.as_str().and_then(|x| Some(String::from(x)));
                c.viewers = viewers.as_u64();
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let mut config = config::read("config");

    let client = reqwest::Client::new();

    let token = perform_auth(&client, &config).await;

    update_channels(&client, token, &mut config).await;

    println!("Live channels:");
    for channel in config.channels {
        if channel.is_online {
            println!("{:#?}", channel);
        }
    }
}
