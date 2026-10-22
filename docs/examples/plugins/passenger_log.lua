-- plugins/passenger_log.lua: writes into game.log what a plugin learns of every passenger
-- boarding the player's bus (the `passenger_ticket` event and `omsi.people.get`) - who they
-- are, where they board and ride to, and the ticket they are about to ask for.

omsi.on("passenger_ticket", function(p)
  omsi.log(string.format("#%d age %.0f %s: %s -> %s (stops %s -> %s), ticket %d %s %.2f",
    p.id, p.age, p.kind, tostring(p.stop_name), tostring(p.destination),
    tostring(p.stop_number), tostring(p.destination_number), p.ticket, p.name, p.price))
  local full = omsi.people.get(p.id)
  if full then
    omsi.log("  model " .. full.model .. ", voice " .. full.voice .. ", state " .. full.state)
  end
end)
