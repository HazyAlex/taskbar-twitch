use std::fmt::Display;
use std::str::FromStr;

use std::sync::Arc;
use std::sync::Mutex;

use enum_iterator::IntoEnumIterator;
use serde::{Deserialize, Deserializer};
use structopt::StructOpt;

pub const DEFAULT_CONFIG_FILE: &'static str = "config.json";

#[derive(Clone, Debug, Default, PartialEq, PartialOrd)]
pub struct Channel {
    pub name: String,
    pub is_online: bool,
    pub title: Option<String>,
    pub viewers: Option<u64>,
}

impl Channel {
    fn from(name: String) -> Self {
        Channel {
            name,
            is_online: false,
            title: None,
            viewers: None,
        }
    }
}

// Deserializing from the command line
impl FromStr for Channel {
    type Err = structopt::clap::Error;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Ok(Channel::from(name.into()))
    }
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
            .ok_or(serde::de::Error::custom("Expected a string"))?;

        Ok(Channel::from(String::from(name)))
    }
}

#[derive(Copy, Clone, Debug, Deserialize, PartialEq, Eq, IntoEnumIterator)]
#[serde(rename_all = "lowercase")]
pub enum OpenStreamUsing {
    Browser,
    Mpv,
    Streamlink,
}

// Used when printing the available players in the GUI.
impl Display for OpenStreamUsing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            OpenStreamUsing::Browser => write!(f, "Browser"),
            OpenStreamUsing::Mpv => write!(f, "Mpv"),
            OpenStreamUsing::Streamlink => write!(f, "Streamlink"),
        }
    }
}

// Deserializing from the command line
impl FromStr for OpenStreamUsing {
    type Err = structopt::clap::Error;

    fn from_str(player: &str) -> Result<Self, Self::Err> {
        match player {
            "browser" => Ok(OpenStreamUsing::Browser),
            "mpv" => Ok(OpenStreamUsing::Mpv),
            "streamlink" => Ok(OpenStreamUsing::Streamlink),
            _ => Err(structopt::clap::Error {
                message: "Couldn't parse the player option.".into(),
                kind: structopt::clap::ErrorKind::ValueValidation,
                info: None,
            }),
        }
    }
}

#[derive(Clone, Debug, StructOpt)]
#[structopt(name = "options")]
struct Arguments {
    #[structopt(short = "c", long = "client")]
    client: Option<String>,

    #[structopt(short = "s", long = "secret")]
    secret: Option<String>,

    #[structopt(short = "p", long = "player")]
    player: Option<OpenStreamUsing>,

    #[structopt(short = "f", long = "file")]
    config_file: Option<String>,

    #[structopt(short = "u", long = "channels", use_delimiter = true)]
    channels: Option<Vec<Channel>>,

    #[structopt(short = "n", long = "notify-titles", use_delimiter = true)]
    notify_title_changed: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct State {
    pub client: String,

    pub secret: String,

    pub player: OpenStreamUsing,

    #[serde(skip)]
    pub session_player: Option<OpenStreamUsing>,

    #[serde(skip)]
    pub config_file: String,

    pub channels: Vec<Channel>,

    #[serde(default)]
    pub notify_title_changed: Vec<String>,
}

impl PartialEq for State {
    fn eq(&self, other: &Self) -> bool {
        if self.client != other.client || self.secret != other.secret {
            return false;
        }

        if self.player != other.player || self.config_file != other.config_file {
            return false;
        }

        if self.notify_title_changed != other.notify_title_changed {
            return false;
        }

        if self.channels.len() != other.channels.len() {
            return false;
        }

        self.channels
            .iter()
            .zip(other.channels.iter())
            .filter(|(a, b)| *a.name != *b.name)
            .count()
            == 0
    }
}

pub fn migrate(config: &Arc<Mutex<State>>, new_config: State) {
    let mut local_config = config.lock().unwrap();

    local_config.client = new_config.client.clone();
    local_config.secret = new_config.secret.clone();
    local_config.player = new_config.player;
    local_config.config_file = new_config.config_file.clone();
    local_config.notify_title_changed = new_config.notify_title_changed.clone();

    // We want to keep the same player that was selected by the user in the current session.
    // local_config.session_player = new_config.session_player;

    // Merge the existing channel information with the new one.
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

fn read_state(filename: &str) -> State {
    let file = std::fs::File::open(filename)
        .expect("Please ensure that there's a valid secret file in the same directory.");
    let reader = std::io::BufReader::new(file);

    let mut state: State = serde_json::from_reader(reader).expect("Valid config format.");

    // As the 'config_file' field is not serialized, we need to set it.
    state.config_file = String::from(filename);

    state
}

pub fn read() -> State {
    if std::env::args().len() <= 1 {
        // Didn't receive any arguments, read the default config file.
        return read_state(DEFAULT_CONFIG_FILE);
    }

    // We have one or more arguments, if they include the 'config_file' field we have to read it,
    //  but the command line arguments have priority over the file.
    let args: Arguments = Arguments::from_args();

    let config: State;
    if let Some(config_file) = &args.config_file {
        config = read_state(config_file.as_str());
    } else {
        config = read_state(DEFAULT_CONFIG_FILE);
    }

    State {
        client: args.client.unwrap_or(config.client),
        secret: args.secret.unwrap_or(config.secret),
        player: args.player.unwrap_or(config.player),

        // The session player will be migrated from the old config,
        //  so we can safely ignore it here.
        session_player: None,

        config_file: args.config_file.unwrap_or(config.config_file),
        channels: args.channels.unwrap_or(config.channels),
        notify_title_changed: args
            .notify_title_changed
            .unwrap_or(config.notify_title_changed),
    }
}
