
# Taskbar Twitch

Windows-only utility that stays in the system tray, it will emit a notification every time a channel goes live.

![](resources/doc_tray_icon.png)

The channels can be changed by editing the configuration file (which you can find available as a shortcut by right-clicking the icon - see the image above), the application will then check for changes and then update the channels accordingly without needing to restart.

After clicking on a channel using the tray icon, the stream will start playing in the video player that was provided to the application by the flags or the configuration file (the stream will be opened in the browser by default). You can also temporarily select a player for the current session in the menu.

### Usage

First you should head to the [Twitch Developers Console](https://dev.twitch.tv/console) page and get a Client ID and Secret Token.

Now you can copy the provided `config.json.example` to `config.json` (the default name for the configuration file) and set the matching fields to the client ID and secret token.
You should also change the channel list to match the ones you are interested in (and the video player application that will be used to open the stream).

### Configuration

#### Flags

* **-c**, **--client**: Twitch Client ID
* **-s**, **--secret**: Twitch Secret Token
* **-p**, **--player**: The video player the app will use to open streams (available players are listed below)
* **-f**, **--file**: Path to the config file (config.json) by default
* **-u**, **--channels**: A list of the channels (comma separated) (e.g. `--channels=j_blow,museun,handmade_hero`)
* **-n**, **--notify-titles**: A list of the channels that will trigger a notification if the title changes (comma separated) (e.g. `--notify-titles=ESL_CSGO`)

These flags are optional and take precedence over the options set in the configuration file.

#### Players

The supported players are:

* Browser
* mpv
* Streamlink
