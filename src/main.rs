mod twitch;
use twitch::State;

use std::sync::Arc;
use std::sync::Mutex;

use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

use trayicon::{MenuBuilder, MenuItem, TrayIconBuilder};

//
// TODO: Better .expect() error messages.
//

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum Events {
    // Tray Icon events
    ClickTrayIcon,
    DoubleClickTrayIcon,
    Exit,
    // User events
    OpenChannelsFile,
    UpdatedChannels,
    OpenChannel(usize), // index of the channel in the config
}

#[tokio::main]
async fn main() {
    let config = Arc::new(Mutex::new(twitch::read_config()));

    let event_loop = EventLoop::<Events>::with_user_event();

    let network_thread_config = config.clone();
    let network_proxy = event_loop.create_proxy();
    tokio::task::spawn_blocking(move || {
        futures::executor::block_on(async {
            twitch::listen_for_events(network_thread_config, &network_proxy).await;
        });
    });

    let file_thread_config = config.clone();
    let file_proxy = event_loop.create_proxy();
    tokio::task::spawn_blocking(move || {
        futures::executor::block_on(async {
            twitch::refresh_config(file_thread_config, &file_proxy).await;
        });
    });

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
        .menu(create_tray_menu(&config))
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
                Events::OpenChannelsFile => {
                    println!("Clicked OpenChannelsFile!");
                }
                Events::OpenChannel(index) => {
                    println!("Clicked {}!", index);
                }
                Events::UpdatedChannels => {
                    tray_icon.set_menu(&create_tray_menu(&config)).ok();
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

    MenuBuilder::new()
        .item("Open channels file", Events::OpenChannelsFile)
        .submenu("Channels", channels)
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

    menu_builder.to_owned()
}
