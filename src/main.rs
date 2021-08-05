use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader};

// TODO: Better .expect() error messages.

struct Config {
    client: String,
    secret: String,
}

fn read_config(filename: &str) -> Config {
    let file = File::open(filename)
        .expect("please ensure that there's a valid secret file in the same directory.");
    let reader = BufReader::new(file);

    let mut config: Config = Config {
        client: String::new(),
        secret: String::new(),
    };

    for line in reader.lines() {
        let line = line.expect("valid line.");
        let parts: Vec<&str> = line.split(":").collect();

        if parts.len() != 2 {
            panic!("Invalid format, example: 'Client:ThisIsAnExampleClientID'.")
        }

        if parts[0].to_lowercase() == "client" {
            config.client.push_str(parts[1]);
        }
        if parts[0].to_lowercase() == "secret" {
            config.secret.push_str(parts[1]);
        }
    }

    if config.client.is_empty() {
        panic!("The client field is required.");
    }
    if config.secret.is_empty() {
        panic!("The secret field is required.");
    }

    return config;
}

async fn perform_auth(client: &reqwest::Client, config: Config) -> String {
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

#[tokio::main]
async fn main() {
    let config = read_config("secret");

    let client = reqwest::Client::new();

    let token = perform_auth(&client, config).await;
    println!("{}", token);
}
