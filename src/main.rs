#![windows_subsystem = "windows"]

mod config;
use config::OpenStreamUsing;
use config::State;

mod twitch;

use std::path::Path;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;

use enum_iterator::IntoEnumIterator;

use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use winrt_notification::Toast;

use trayicon::{MenuBuilder, MenuItem, TrayIconBuilder};

// Used to track releases - it's available in the traybar so that the user knows what version they currently have.
const APP_VERSION: &'static str = "Version 1.0.3";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Events {
    // Tray Icon events
    ClickTrayIcon,
    DoubleClickTrayIcon,
    Exit,
    // User events
    OpenChannelsFile,
    UpdatedChannels,
    ChangeCurrentPlayer(OpenStreamUsing),
    OpenChannel(usize), // index of the channel in the config
}

#[tokio::main]
async fn main() {
    set_panic_hook();

    let state = Arc::new(Mutex::new(config::read()));

    let event_loop = EventLoop::<Events>::with_user_event();

    // After reading the config (and if there are changes), notify the network thread
    //  using this channel so it can fetch the updates for the newly added channels.
    let (tx, rx) = mpsc::channel();

    let network_thread_state = state.clone();
    let network_proxy = event_loop.create_proxy();
    tokio::task::spawn_blocking(move || {
        futures::executor::block_on(async {
            twitch::listen_for_events(network_thread_state, &network_proxy, rx).await;
        });
    });

    let file_thread_state = state.clone();
    let file_proxy = event_loop.create_proxy();
    tokio::task::spawn_blocking(move || {
        futures::executor::block_on(async {
            twitch::refresh_config(file_thread_state, &file_proxy, tx).await;
        });
    });

    let event_loop_state = state.clone();
    run_event_loop(event_loop, event_loop_state);
}

fn run_event_loop(event_loop: EventLoop<Events>, state: Arc<Mutex<State>>) {
    let window = WindowBuilder::new()
        .with_visible(false)
        .build(&event_loop)
        .expect("Valid window.");

    let mut tray_icon = TrayIconBuilder::new()
        .sender_winit(event_loop.create_proxy())
        .icon_from_buffer(include_bytes!("../resources/twitch.ico"))
        .tooltip("Taskbar Twitch")
        .on_click(Events::ClickTrayIcon)
        .on_double_click(Events::DoubleClickTrayIcon)
        .menu(create_tray_menu(&state))
        .build()
        .expect("Couldn't create a tray icon menu!");

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
                Events::OpenChannelsFile => {
                    let local_state = state.lock().unwrap();

                    open::that(local_state.config_file.as_str()).ok();
                }
                Events::OpenChannel(index) => {
                    let local_state = state.lock().unwrap();

                    let current_player = local_state.session_player.unwrap_or(local_state.player);

                    match current_player {
                        config::OpenStreamUsing::Browser => {
                            let mut result = String::from("https://twitch.tv/");
                            result.push_str(local_state.channels[index].name.as_str());

                            open::that(result).unwrap();
                        }
                        config::OpenStreamUsing::Mpv => {
                            let mut args = String::from("https://twitch.tv/");
                            args.push_str(local_state.channels[index].name.as_str());
                            args.push_str(" --ytdl-format=best");

                            open::with(args, "mpv").unwrap();
                        }
                        config::OpenStreamUsing::Streamlink => {
                            let mut args = String::from("twitch.tv/");
                            args.push_str(local_state.channels[index].name.as_str());
                            args.push_str(" best");

                            open::with(args, "streamlink").unwrap();
                        }
                    }
                }
                Events::UpdatedChannels => {
                    tray_icon.set_menu(&create_tray_menu(&state)).ok();
                }
                Events::ChangeCurrentPlayer(player) => {
                    {
                        let mut local_state = state.lock().unwrap();

                        local_state.session_player = Some(player);
                    }

                    // We need to drop the mutex, and now the GUI can be updated.
                    tray_icon.set_menu(&create_tray_menu(&state)).ok();
                }
                Events::Exit => *control_flow = ControlFlow::Exit,
                _ => {}
            },
            _ => (),
        }
    });
}

fn create_tray_menu(config: &Arc<Mutex<State>>) -> MenuBuilder<Events> {
    let channels = create_channels_menu(&config);
    let players = create_players_menu(&config);

    MenuBuilder::new()
        .with(MenuItem::Item {
            name: String::from(APP_VERSION),
            disabled: true,
            id: Events::ClickTrayIcon,
            icon: None,
        })
        .item("Open channels file", Events::OpenChannelsFile)
        .submenu("Channels", channels)
        .submenu("Player", players)
        .separator()
        .item("E&xit", Events::Exit)
}

fn create_channels_menu(config: &Arc<Mutex<State>>) -> MenuBuilder<Events> {
    let mut menu_builder: MenuBuilder<Events> = MenuBuilder::new();

    let config = config.lock().unwrap();

    for (index, channel) in config.channels.iter().enumerate() {
        let mut result = channel.name.to_string();

        if channel.is_online {
            //result.push_str(" (LIVE)");

            if let Some(title) = &channel.title {
                result.push_str(" - ");
                result.push_str(title.as_str());
            };

            if let Some(viewers) = channel.viewers {
                result.push_str(" (");
                result.push_str(viewers.to_string().as_str());
                result.push_str(" viewers)");
            };
        }

        menu_builder = menu_builder.clone().with(MenuItem::Item {
            id: Events::OpenChannel(index),
            name: result,
            disabled: !channel.is_online,
            icon: None,
        });
    }

    menu_builder
}

fn create_players_menu(config: &Arc<Mutex<State>>) -> MenuBuilder<Events> {
    let mut menu_builder: MenuBuilder<Events> = MenuBuilder::new();

    let config = config.lock().unwrap();

    for player in OpenStreamUsing::into_enum_iter() {
        // If we already selected a player for the current session, use it.
        // Otherwise, use the player provided by the arguments/config.
        let is_selected = if let Some(session_player) = config.session_player {
            session_player == player
        } else {
            config.player == player
        };

        let event = Events::ChangeCurrentPlayer(player);

        menu_builder = menu_builder.checkable(&player.to_string(), is_selected, event);
    }

    menu_builder
}

fn send_notification(title: &str, text: &str) {
    let icon_path = std::fs::canonicalize("./resources/twitch.ico")
        .map(|path| remove_extended_path_prefix(path))
        .unwrap_or_default();

    // As we don't have an 'AppUserModeID', we'll just steal an appropriate one.
    Toast::new("Microsoft.Windows.MediaPlayer32")
        .icon(
            &Path::new(&icon_path),
            winrt_notification::IconCrop::Circular,
            "application icon",
        )
        .title(title)
        .text1(text)
        .sound(Some(winrt_notification::Sound::Reminder))
        .duration(winrt_notification::Duration::Short)
        .show()
        .expect("Unable to create the notification.");
}

fn remove_extended_path_prefix(path: PathBuf) -> String {
    const PREFIX: &str = r#"\\?\"#;

    let p = path.display().to_string();

    if p.starts_with(PREFIX) {
        p[PREFIX.len()..].to_string()
    } else {
        p
    }
}

fn set_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let mut message = String::new();

        if let Some(s) = info.payload().downcast_ref::<&str>() {
            message.push_str(s);
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            message.push_str(s);
        } else {
            message.push_str("Unknown error.");
        }

        if let Some(location) = info.location() {
            message.push_str(
                format!(" (occurred at: '{}':{})", location.file(), location.line()).as_str(),
            );
        }

        send_notification("A runtime error occurred.", message.as_str());

        std::process::exit(1)
    }));
}
