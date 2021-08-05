use serde::{Deserialize, Deserializer};

#[derive(Debug)]
pub struct Channel {
    pub name: String,
    pub is_online: bool,
    pub title: Option<String>,
    pub viewers: Option<u64>,
}

#[derive(Deserialize)]
pub struct Config {
    pub client: String,
    pub secret: String,
    pub channels: Vec<Channel>,
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

pub fn read(filename: &str) -> Config {
    let file = std::fs::File::open(filename)
        .expect("please ensure that there's a valid secret file in the same directory.");
    let reader = std::io::BufReader::new(file);

    return serde_json::from_reader(reader).expect("valid config format.");
}
