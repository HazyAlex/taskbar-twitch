mod config;

use std::thread;

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

async fn update_channels(client: &reqwest::Client, token: &String, config: &mut Config) {
    let mut url = String::from("https://api.twitch.tv/helix/streams?");

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
        let name = name.as_str().expect("expected to get an username.");

        for c in &mut config.channels {
            if c.name == name {
                c.is_online = true;
                c.title = title.as_str().map(|x| String::from(x));
                c.viewers = viewers.as_u64();
            }
        }
    }
}

use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

use trayicon::{MenuBuilder, TrayIconBuilder};

#[derive(Clone, Eq, PartialEq, Debug)]
enum Events {
    ClickTrayIcon,
    DoubleClickTrayIcon,
    Exit,
    OpenChannelsFile,
    Active,
}

#[tokio::main]
async fn main() {
    tokio::spawn(async {
        let mut config = config::read("config");

        let client = reqwest::Client::new();
        let token = perform_auth(&client, &config).await;

        loop {
            update_channels(&client, &token, &mut config).await;

            println!("Live channels:");
            for channel in &config.channels {
                if channel.is_online {
                    println!("{:#?}", channel);
                }
            }

            std::thread::sleep(std::time::Duration::from_secs(60));
        }
    });

    run_event_loop();
}

fn run_event_loop() {
    let event_loop = EventLoop::<Events>::with_user_event();
    let window = WindowBuilder::new()
        .with_visible(false)
        .build(&event_loop)
        .expect("valid window.");

    let mut tray_icon = TrayIconBuilder::new()
        .sender_winit(event_loop.create_proxy())
        .icon_from_buffer(include_bytes!("../resources/icon.ico"))
        .tooltip("Taskbar Twitch")
        .on_click(Events::ClickTrayIcon)
        .on_double_click(Events::DoubleClickTrayIcon)
        .menu(
            MenuBuilder::new()
                .checkable("Active", true, Events::Active)
                .item("Open channels file", Events::OpenChannelsFile)
                .separator()
                .item("E&xit", Events::Exit),
        )
        .build()
        .expect("Couldn't create a tray icon menu!");

    //
    // Event loop
    //
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        // Tray icon uses normal message pump from winit, for orderly closure
        // and removal of the tray icon when you exit it must be moved inside the main loop.
        let _ = tray_icon;

        match event {
            // Main window events
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                window_id,
            } if window_id == window.id() => {
                *control_flow = ControlFlow::Exit;
            }

            // User events
            Event::UserEvent(e) => match e {
                Events::Active => {
                    if let Some(old_value) = tray_icon.get_menu_item_checkable(Events::Active) {
                        let active = !old_value;

                        tray_icon
                            .set_menu_item_checkable(Events::Active, active)
                            .ok();

                        // TODO: Start/stop new requests.
                    }
                }
                Events::OpenChannelsFile => {
                    println!("Clicked OpenChannelsFile!");
                }
                Events::Exit => *control_flow = ControlFlow::Exit,
                _ => {}
            },
            _ => (),
        }
    });
}
