// The "playing now" counter of openOMSI: a running game says every three minutes that it is
// being played (crates/omsi-app/src/presence.rs; every ten minutes, every three in games
// before 0.1.1552), the website and the README badge read how many are. One Durable Object
// keeps the sessions (a random id each, its system and the game's version, the time of its
// last word) and forgets one 25 minutes after that. Everything has to fit the free plan's
// 100 000 requests a day: over it Cloudflare answers every request with error 1027 until
// midnight UTC.
// No address and nothing else about a player is kept.
//
//   POST /ping  {"id": "<32 hex>", "v": "0.1.1512", "os": "windows"}   -> 204
//   POST /bye   {"id": "<32 hex>"}                                       -> 204
//   GET  /players -> {"players": 12, "systems": {"windows": 9, ...}, "updated": "..."}
//   GET  /badge   -> the same count for a shields.io endpoint badge

import { DurableObject } from "cloudflare:workers";

const ALIVE_MS = 25 * 60 * 1000;
const SYSTEMS = ["windows", "macos", "linux", "android"];

export class Presence extends DurableObject {
  constructor(ctx, env) {
    super(ctx, env);
    this.sql = ctx.storage.sql;
    this.sql.exec("CREATE TABLE IF NOT EXISTS sessions (id TEXT PRIMARY KEY, seen INTEGER NOT NULL, os TEXT NOT NULL, v TEXT NOT NULL)");
  }

  forget(now) {
    this.sql.exec("DELETE FROM sessions WHERE seen < ?", now - ALIVE_MS);
  }

  async ping(id, os, v) {
    const now = Date.now();
    this.sql.exec("INSERT INTO sessions (id, seen, os, v) VALUES (?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET seen = excluded.seen, os = excluded.os, v = excluded.v", id, now, os, v);
  }

  async bye(id) {
    this.sql.exec("DELETE FROM sessions WHERE id = ?", id);
  }

  async count() {
    this.forget(Date.now());
    const systems = {};
    for (const row of this.sql.exec("SELECT os, COUNT(*) AS n FROM sessions GROUP BY os")) {
      systems[row.os] = row.n;
    }
    const players = Object.values(systems).reduce((a, b) => a + b, 0);
    return { players, systems, updated: new Date().toISOString() };
  }
}

const CORS = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Methods": "GET, POST, OPTIONS",
  "Access-Control-Allow-Headers": "Content-Type",
};

function json(body, status = 200, extra = {}) {
  return new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json", ...CORS, ...extra } });
}

async function body(request) {
  try {
    const b = await request.json();
    return b && typeof b === "object" ? b : null;
  } catch {
    return null;
  }
}

const validId = (id) => typeof id === "string" && /^[0-9a-f]{32}$/.test(id);

export default {
  async fetch(request, env, ctx) {
    const url = new URL(request.url);
    const counter = env.PRESENCE.get(env.PRESENCE.idFromName("openomsi"));
    if (request.method === "OPTIONS") {
      return new Response(null, { status: 204, headers: CORS });
    }
    if (request.method === "POST" && url.pathname === "/ping") {
      const b = await body(request);
      if (!b || !validId(b.id)) return json({ error: "bad id" }, 400);
      const os = SYSTEMS.includes(b.os) ? b.os : "other";
      const v = typeof b.v === "string" ? b.v.slice(0, 32) : "";
      await counter.ping(b.id, os, v);
      return new Response(null, { status: 204, headers: CORS });
    }
    if (request.method === "POST" && url.pathname === "/bye") {
      const b = await body(request);
      if (b && validId(b.id)) await counter.bye(b.id);
      return new Response(null, { status: 204, headers: CORS });
    }
    if (request.method === "GET" && (url.pathname === "/players" || url.pathname === "/badge")) {
      // (read at most every two minutes from the counter, and cached by browsers and
      // shields.io as long: the website and the badge can be asked as often as anybody likes)
      const cache = caches.default;
      const key = new Request(url.origin + url.pathname);
      const hit = await cache.match(key);
      if (hit) return hit;
      const c = await counter.count();
      const out = url.pathname === "/badge"
        ? json({ schemaVersion: 1, label: "playing now", message: String(c.players), color: c.players > 0 ? "brightgreen" : "lightgrey", cacheSeconds: 300 }, 200, { "Cache-Control": "public, max-age=120" })
        : json(c, 200, { "Cache-Control": "public, max-age=120" });
      ctx.waitUntil(cache.put(key, out.clone()));
      return out;
    }
    return new Response("openOMSI presence: GET /players, GET /badge\n", { status: url.pathname === "/" ? 200 : 404, headers: { "Content-Type": "text/plain", ...CORS } });
  },
};
