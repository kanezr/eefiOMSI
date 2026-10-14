# openOMSI Voice (GreenTeaSpeak plugin)

Positional voice for openOMSI multiplayer through [GreenTeaSpeak 2](https://greenteaspeak.de),
the way SaltyChat does it for FiveM: you hear the other players from where they stand or
sit, quieter with distance, and muffled when one of you is in a bus and the other is not.

## For players

1. Pack the plugin: `./package.sh` (makes `openomsi-voice.zip`).
2. In GreenTeaSpeak: **Options → Extensions → Plugins → Install ZIP…** and choose it.
3. Connect GreenTeaSpeak to the voice server your openOMSI server names, then join the
   openOMSI server. The plugin moves you into the in-game channel and renames you
   `<name> #<player id>` so that the other games know who is who.

openOMSI shows a line on the HUD while the voice chat is not ready (on another voice
server, microphone muted; GreenTeaSpeak not running: for the first ten seconds only). *Settings → General → Voice chat
through GreenTeaSpeak* switches it off in the game.

## For server owners

In `server.cfg` (a dedicated server) or `~/.openomsi/voice.cfg` (a game hosting by code):

```
voice_server_uid = abcdEFGH1234abcdEFGH1234abc=
voice_channel = 12
voice_channel_password =
voice_range = 20
```

`voice_server_uid` is the unique id from GreenTeaSpeak's server info panel (needed: the
plugin moves nobody on a voice server the session does not name), `voice_channel` the
in-game channel's id or name (empty: no voice chat), `voice_range` how far a player is heard
in metres. The channel's password is sent to every player who joins.

## Protocol

The plugin listens on `127.0.0.1:38088` (its storage key `port` changes that; this machine
only). The game connects and sends one JSON object a line.

The first line is `hello` with `key`: the contents of `~/.openomsi/voice-plugin.key`, which
the game makes the first time it looks for the plugin (the storage key `keyFile` names
another file). A connection whose first line is not that - a web page posting to the port,
anything that looks like HTTP - is hung up on before anything of it is done. The plugin
answers `welcome`, or `refused` (`error`) and hangs up when a game is linked already: one
game at a time.

| `type` | fields | what the plugin does |
|---|---|---|
| `hello` | `key` | checks the key; `welcome` |
| `initiate` | `serverUid`, `channel`, `password`, `nickname`, `range` | checks the server's unique id (refused when empty), renames the user, moves them into the channel, switches 3D voice on |
| `self` | `x`, `y`, `z`, `yaw` | `setListenerPose` (`yaw` = `yawDeg`, SaltyChat's rotation: 0 north, counter-clockwise) |
| `players` | `players: [{nickname, x, y, z, range, volume}]` | `findClientByName` (looked up again every 10 s: a player who came back has a new client id) → `setClientPose` (`volume`: `volumeOverride`, null for the distance fall-off); players no longer listed get `clearClientPose` |
| `reset` | | `resetSpatial`, 3D voice off, the user's nickname and channel as before the session |

Positions are metres, z up, relative to a point near where the game started (map coordinates
run into the millions). The plugin answers with `state` (`inChannel`, `error`), `talk`
(`nickname`, `talking`) and `mute` (`microphoneMuted`, `soundMuted`).

The plugin is plain JavaScript (ES module, `node:net`, no dependencies): GreenTeaSpeak's
`@greentea/plugin-sdk` is only needed for its types when writing a plugin. The game's side is
`crates/omsi-app/src/voice.rs`.
