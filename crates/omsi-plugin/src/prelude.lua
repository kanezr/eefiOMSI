-- The Lua side of the `omsi` table: events, timers, watches and saved data.
-- (the Rust side, src/lua.rs, has put the functions that reach the game into `omsi`)
local omsi = omsi
local handlers = {}      -- event -> list of functions
local timers = {}        -- id -> {at, every, fn}
local next_id = 1
local watches = {}       -- id -> {kind, name, fn, last}
local clock = 0
local last_info = {}

function omsi.on(event, fn)
  assert(type(event) == "string", "omsi.on: the event is a name")
  assert(type(fn) == "function", "omsi.on: the handler is a function")
  local list = handlers[event]
  if not list then list = {}; handlers[event] = list end
  list[#list + 1] = fn
  return fn
end

function omsi.off(event, fn)
  local list = handlers[event]
  if not list then return end
  for i = #list, 1, -1 do
    if list[i] == fn then table.remove(list, i) end
  end
end

function omsi.after(seconds, fn)
  local id = next_id; next_id = id + 1
  timers[id] = { at = clock + seconds, fn = fn }
  return id
end

function omsi.every(seconds, fn)
  assert(seconds > 0, "omsi.every: the interval must be above 0")
  local id = next_id; next_id = id + 1
  timers[id] = { at = clock + seconds, every = seconds, fn = fn }
  return id
end

-- omsi.watch("var", name, fn(new, old)) / omsi.watch(name, fn): fn runs when the value changes
function omsi.watch(kind, name, fn)
  if fn == nil then kind, name, fn = "var", kind, name end
  assert(kind == "var" or kind == "str" or kind == "sys", "omsi.watch: kind is var, str or sys")
  local id = next_id; next_id = id + 1
  watches[id] = { kind = kind, name = name, fn = fn }
  return id
end

function omsi.cancel(id)
  timers[id] = nil
  watches[id] = nil
end

function omsi.time() return clock end

function omsi.emit(event, ...)
  local list = handlers[event]
  if list then
    for _, fn in ipairs({ table.unpack(list) }) do fn(...) end
  end
  -- a global function of the same name: on_frame, on_start, ...
  local g = rawget(_G, "on_" .. event)
  if type(g) == "function" then g(...) end
end

local read = { var = omsi.var, str = omsi.str, sys = omsi.sys }

function omsi._tick(dt)
  clock = clock + dt
  -- timers due, in id order (a timer may add or cancel others)
  local due = {}
  for id, t in pairs(timers) do
    if t.at <= clock then due[#due + 1] = id end
  end
  table.sort(due)
  for _, id in ipairs(due) do
    local t = timers[id]
    if t then
      if t.every then t.at = t.at + t.every; if t.at <= clock then t.at = clock + t.every end
      else timers[id] = nil end
      t.fn()
    end
  end
  for _, w in pairs(watches) do
    local v = read[w.kind](w.name)
    if v ~= w.last then
      local old = w.last
      w.last = v
      if old ~= nil or v ~= nil then w.fn(v, old) end
    end
  end
  -- the keys pressed since the last frame, then what changed in the game's state
  for _, k in ipairs(omsi._keys()) do omsi.emit("key", k[1], k[2]) end
  if handlers.next_stop or handlers.view or handlers.duty or rawget(_G, "on_next_stop") or rawget(_G, "on_view") or rawget(_G, "on_duty") then
    local i = omsi.info()
    -- (also to a stop of the same name: the two sides of a road often share one)
    if i.next_stop ~= nil and (i.next_stop ~= last_info.next_stop or i.next_stop_number ~= last_info.next_stop_number) then
      omsi.emit("next_stop", i.next_stop, last_info.next_stop)
    end
    if i.view ~= last_info.view then omsi.emit("view", i.view, last_info.view) end
    if (i.line or "") .. "/" .. (i.tour or "") ~= (last_info.line or "") .. "/" .. (last_info.tour or "") then omsi.emit("duty", i.line, i.tour) end
    last_info = i
  end
  omsi.emit("frame", dt)
end

-- small helpers
function omsi.speed()
  return math.abs(omsi.var("Velocity") or 0)
end

function omsi.distance(x, y)
  local px, py = omsi.position()
  if not px then return nil end
  return math.sqrt((px - x) ^ 2 + (py - y) ^ 2)
end

function omsi.clock()
  local t = math.floor(omsi.info().clock or 0)
  return string.format("%02d:%02d:%02d", t // 3600 % 24, t // 60 % 60, t % 60)
end

-- saved data: omsi.data is written on the way out and read back on the next start
local function dump(v, indent, seen)
  local t = type(v)
  if t == "string" then return string.format("%q", v) end
  if t == "number" then
    if v ~= v then return "0/0" end
    if v == math.huge then return "math.huge" end
    if v == -math.huge then return "-math.huge" end
    return string.format("%.17g", v)
  end
  if t == "boolean" or t == "nil" then return tostring(v) end
  if t ~= "table" then return "nil" end
  if seen[v] then return "nil" end
  seen[v] = true
  local keys = {}
  for k in pairs(v) do
    local kt = type(k)
    if kt == "string" or kt == "number" or kt == "boolean" then keys[#keys + 1] = k end
  end
  table.sort(keys, function(a, b)
    if type(a) == type(b) then return a < b end
    return type(a) < type(b)
  end)
  local inner = indent .. "  "
  local out = { "{\n" }
  for _, k in ipairs(keys) do
    local ks
    if type(k) == "string" and k:match("^[%a_][%w_]*$") then ks = k
    else ks = "[" .. dump(k, inner, seen) .. "]" end
    out[#out + 1] = inner .. ks .. " = " .. dump(v[k], inner, seen) .. ",\n"
  end
  out[#out + 1] = indent .. "}"
  seen[v] = nil
  return table.concat(out)
end

function omsi._save()
  if next(omsi.data) == nil then
    omsi._write_data(nil)
  else
    omsi._write_data("return " .. dump(omsi.data, "", {}) .. "\n")
  end
end

function omsi.save() omsi._save() end

do
  local text = omsi._read_data()
  omsi.data = {}
  if text then
    local chunk = load(text, "=saved data", "t", { math = { huge = math.huge } })
    local ok, t = pcall(chunk or error)
    if ok and type(t) == "table" then omsi.data = t end
  end
end
