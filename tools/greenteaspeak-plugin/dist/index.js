// openOMSI Voice - a GreenTeaSpeak 2 plugin (@greentea/plugin-sdk) for positional voice in
// openOMSI multiplayer, the way SaltyChat does it for FiveM.
//
// The game (crates/omsi-app/src/voice.rs) connects to 127.0.0.1:38088 and sends one JSON
// object a line:
//   hello     {key}: first, the key of ~/.openomsi/voice-plugin.key (the game makes it).
//             Whatever else connects (a web page posting to the port, another program) is
//             hung up on; one game is linked at a time. Answered with welcome / refused.
//   initiate  {serverUid, channel, password, nickname, range}: go to the session's channel
//   self      {x, y, z, yaw}: the listener (yaw in degrees, SaltyChat's Rotation)
//   players   {players: [{nickname, x, y, z, range, volume}]}: everybody else who can be heard
//   reset     the session is over: 3D voice off, the nickname and channel as before
// and is told back:
//   state     {connected, inChannel, error}
//   talk      {nickname, talking}
//   mute      {microphoneMuted, soundMuted}
//
// Plain JavaScript (ES module, no dependencies, no build step): the SDK's types are only
// needed to write a plugin, not to run one.

import net from "node:net";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import crypto from "node:crypto";

const DEFAULT_PORT = 38088;
/** A nickname not found in the channel is looked for again after this long (ms). */
const MISS_RETRY_MS = 2000;
/** A client found in the channel is looked for again after this long (ms): one who left and
 * came back to the voice server has another client id. */
const HIT_RETRY_MS = 10000;
/** One line from the game is at most this long (a players message of 64 players fits). */
const MAX_LINE = 64 * 1024;
/** The game's hello is at most this long and comes within this long (ms). */
const MAX_HELLO = 1024;
const HELLO_MS = 5000;

let ctx = null;
let server = null;
/** The one game linked (it said hello with the key), null when none. */
let game = null;
/** The key file: storage key "keyFile", else ~/.openomsi/voice-plugin.key. */
let keyFile = "";
const unsubscribe = [];

/** The session the game asked for (its initiate), null when none. */
let session = null;
let inChannel = false;
let problem = "";
/** nickname -> {clientId, at}; clientId null for a nickname not found (looked for again later). */
const clients = new Map();
/** Client ids given a 3D pose, to clear when they leave. */
let posed = new Set();
/** The user's nickname and channel before the session moved them: back there after it. */
let before = null;

function send(sock, msg) {
  if (!sock.destroyed) sock.write(JSON.stringify(msg) + "\n");
}

function broadcast(msg) {
  if (game) send(game, msg);
}

function defaultKeyFile() {
  // (the game's home: HOME first, as it looks for it)
  const home = process.env.HOME || process.env.USERPROFILE || os.homedir();
  return path.join(home, ".openomsi", "voice-plugin.key");
}

/** The game's key is this one (read afresh: the game makes the file when it first runs). */
function keyMatches(given) {
  let key;
  try {
    key = fs.readFileSync(keyFile, "utf8").trim();
  } catch (e) {
    ctx.log.warn("no key file at " + keyFile + " (start openOMSI once)");
    return false;
  }
  if (key.length < 32 || typeof given !== "string") return false;
  const a = Buffer.from(key);
  const b = Buffer.from(given);
  return a.length === b.length && crypto.timingSafeEqual(a, b);
}

/** A web browser's request (a page posting to the port), not a game. */
function looksLikeHttp(text) {
  return /^[A-Z]+ \S/.test(text) || text.includes("HTTP/");
}

/** The user's own nickname and channel, as far as the client tells them. */
async function whereAmI() {
  const c = ctx.connection;
  let nickname = null;
  let channel = null;
  try {
    const st = await c.getState();
    nickname = st.nickname ?? st.ownNickname ?? null;
    channel = st.channelId ?? (st.channel && st.channel.id) ?? null;
  } catch (_) {}
  try {
    if (nickname === null && typeof c.getOwnNickname === "function") nickname = await c.getOwnNickname();
    if (channel === null && typeof c.getOwnChannel === "function") {
      const ch = await c.getOwnChannel();
      channel = ch && typeof ch === "object" ? ch.id ?? ch.channelId ?? null : ch;
    }
  } catch (e) {
    ctx.log.warn("could not tell the user's nickname or channel", e);
  }
  return { nickname, channel };
}

function report() {
  broadcast({ type: "state", connected: true, inChannel, error: problem });
}

/** A channel as GreenTeaSpeak takes it: an id when it is a number, else its name. */
function channelRef(channel) {
  return /^\d+$/.test(String(channel)) ? Number(channel) : String(channel);
}

async function initiate() {
  if (!session) return;
  inChannel = false;
  problem = "";
  try {
    const state = await ctx.connection.getState();
    if (!state.connected) {
      problem = "GreenTeaSpeak is not connected to a voice server";
      return report();
    }
    // (a session that names no voice server would move the user on whichever one they are
    // on: their clan's, a private one)
    if (!session.serverUid) {
      problem = "the server names no voice server (voice_server_uid)";
      return report();
    }
    const uid = await ctx.connection.getServerUid();
    if (uid !== session.serverUid) {
      problem = "GreenTeaSpeak is on another voice server than the session's";
      return report();
    }
    if (!before) before = await whereAmI();
    let nickProblem = "";
    try {
      await ctx.connection.setOwnNickname(session.nickname);
    } catch (e) {
      ctx.log.warn("could not take the nickname", session.nickname, e);
      // (the others cannot tell who is who without it: their games pose nobody for us)
      nickProblem = "could not take the nickname " + session.nickname + ": " + (e && e.message ? e.message : String(e));
    }
    await ctx.connection.moveSelfToChannel(channelRef(session.channel), session.password || undefined);
    await ctx.voice.setSpatialEnabled(true);
    clients.clear();
    inChannel = true;
    problem = nickProblem;
    ctx.log.info("in the session's channel", session.channel, "as", session.nickname);
  } catch (e) {
    problem = "could not join the voice channel: " + (e && e.message ? e.message : String(e));
    ctx.log.error(problem);
  }
  report();
}

async function reset() {
  const was = before;
  session = null;
  inChannel = false;
  before = null;
  clients.clear();
  posed = new Set();
  try {
    await ctx.voice.resetSpatial();
    await ctx.voice.setSpatialEnabled(false);
  } catch (e) {
    ctx.log.warn("reset", e);
  }
  // the user as they were before the session: their nickname, their channel
  if (was) {
    try {
      if (was.nickname) await ctx.connection.setOwnNickname(was.nickname);
    } catch (e) {
      ctx.log.warn("could not take the nickname back", was.nickname, e);
    }
    try {
      if (was.channel !== null && was.channel !== undefined) await ctx.connection.moveSelfToChannel(channelRef(was.channel));
    } catch (e) {
      ctx.log.warn("could not go back to the channel", was.channel, e);
    }
  }
}

async function clientIdOf(nickname) {
  const known = clients.get(nickname);
  const now = Date.now();
  if (known && now - known.at < (known.clientId !== null ? HIT_RETRY_MS : MISS_RETRY_MS)) return known.clientId;
  let clientId = null;
  try {
    const c = await ctx.connection.findClientByName(nickname);
    clientId = c ? c.clientId : null;
  } catch (e) {
    ctx.log.warn("findClientByName", nickname, e);
  }
  clients.set(nickname, { clientId, at: now });
  return clientId;
}

async function placeSelf(m) {
  await ctx.voice.setListenerPose({ x: m.x, y: m.y, z: m.z, yawDeg: m.yaw });
}

async function placePlayers(m) {
  const now = new Set();
  for (const p of m.players || []) {
    const id = await clientIdOf(String(p.nickname));
    if (id === null || id === undefined) continue;
    now.add(id);
    await ctx.voice.setClientPose(id, {
      x: p.x,
      y: p.y,
      z: p.z,
      voiceRange: p.range,
      alive: true,
      volumeOverride: typeof p.volume === "number" ? p.volume : null,
    });
  }
  // (a player gone from the session or out of reach: heard as without the plugin no more)
  for (const id of posed) {
    if (!now.has(id)) {
      try {
        await ctx.voice.clearClientPose(id);
      } catch (e) {
        ctx.log.warn("clearClientPose", id, e);
      }
    }
  }
  posed = now;
}

// The game sends ten positions a second; GreenTeaSpeak's calls are asynchronous. One of each
// kind is worked on at a time and only the latest waiting one is kept.
const latest = { self: null, players: null };
const busy = { self: false, players: false };

function queue(kind, msg, work) {
  latest[kind] = msg;
  if (busy[kind]) return;
  busy[kind] = true;
  (async () => {
    while (latest[kind]) {
      const m = latest[kind];
      latest[kind] = null;
      if (!inChannel) continue;
      try {
        await work(m);
      } catch (e) {
        ctx.log.warn(kind, e);
      }
    }
    busy[kind] = false;
  })();
}

function onMessage(sock, m) {
  switch (m.type) {
    case "initiate": {
      const next = {
        serverUid: String(m.serverUid || ""),
        channel: String(m.channel || ""),
        password: String(m.password || ""),
        nickname: String(m.nickname || ""),
        range: Number(m.range) || 20,
      };
      const same = session && JSON.stringify(session) === JSON.stringify(next);
      session = next;
      if (same && inChannel) send(sock, { type: "state", connected: true, inChannel, error: problem });
      else initiate();
      break;
    }
    case "self":
      queue("self", m, placeSelf);
      break;
    case "players":
      queue("players", m, placePlayers);
      break;
    case "reset":
      reset();
      break;
    default:
      break;
  }
}

/** The first line of a connection: the game's hello with its key, or a hang-up. */
function onHello(sock, line) {
  let m = null;
  try {
    m = JSON.parse(line);
  } catch (_) {}
  if (!m || m.type !== "hello" || !keyMatches(m.key)) {
    ctx.log.warn("a connection that is not openOMSI's (no hello with its key): hung up");
    sock.destroy();
    return false;
  }
  if (game && !game.destroyed) {
    send(sock, { type: "refused", error: "another openOMSI game is already linked to GreenTeaSpeak" });
    sock.end();
    return false;
  }
  game = sock;
  ctx.log.info("openOMSI connected");
  send(sock, { type: "welcome" });
  return true;
}

function onGame(sock) {
  sock.setEncoding("utf8");
  sock.setNoDelay(true);
  let linked = false;
  // (no hello in time: not the game)
  const timer = setTimeout(() => {
    if (!linked) sock.destroy();
  }, HELLO_MS);
  let buf = "";
  sock.on("data", (chunk) => {
    if (sock.destroyed) return;
    buf += chunk;
    if (!linked) {
      // (a browser posting to the port, or something that is not the game at all: gone
      // before a line of it is read)
      if (looksLikeHttp(buf) || (buf.length > MAX_HELLO && buf.indexOf("\n") < 0) || buf[0] !== "{") {
        ctx.log.warn("a connection that is not openOMSI's: hung up");
        sock.destroy();
        return;
      }
    } else if (buf.length > MAX_LINE && buf.indexOf("\n") < 0) {
      buf = "";
      return;
    }
    let nl;
    while ((nl = buf.indexOf("\n")) >= 0) {
      const line = buf.slice(0, nl).trim();
      buf = buf.slice(nl + 1);
      if (!linked) {
        if (!onHello(sock, line)) return;
        linked = true;
        clearTimeout(timer);
        continue;
      }
      if (!line) continue;
      try {
        onMessage(sock, JSON.parse(line));
      } catch (e) {
        ctx.log.warn("bad line from openOMSI", e);
      }
    }
  });
  const gone = () => {
    clearTimeout(timer);
    if (game !== sock) return;
    game = null;
    ctx.log.info("openOMSI disconnected");
    reset();
  };
  sock.on("close", gone);
  sock.on("error", gone);
}

export default {
  async activate(context) {
    ctx = context;
    const port = Number(await ctx.storage.get("port")) || DEFAULT_PORT;
    keyFile = String((await ctx.storage.get("keyFile")) || "") || defaultKeyFile();
    unsubscribe.push(
      ctx.events.onTalkState((ev) => broadcast({ type: "talk", nickname: ev.name, talking: ev.talking })),
      ctx.events.onMuteState((ev) => broadcast({ type: "mute", microphoneMuted: ev.microphoneMuted, soundMuted: ev.soundMuted })),
      ctx.events.onConnectionChange((ev) => {
        clients.clear();
        if (ev.connected) initiate();
        else {
          inChannel = false;
          problem = "GreenTeaSpeak is not connected to a voice server";
          report();
        }
      }),
    );
    server = net.createServer(onGame);
    server.on("error", (e) => ctx.log.error("cannot listen on 127.0.0.1:" + port, e));
    // (this machine only: a game elsewhere has no business moving us about)
    server.listen(port, "127.0.0.1", () => ctx.log.info("waiting for openOMSI on 127.0.0.1:" + port));
  },

  async deactivate() {
    for (const u of unsubscribe.splice(0)) {
      try {
        u();
      } catch (_) {}
    }
    if (game) game.destroy();
    game = null;
    if (server) server.close();
    server = null;
    await reset();
  },
};
