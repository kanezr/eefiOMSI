-- plugins/reduced_tickets.lua: every adult buying a full-rate ticket asks for the reduced one
-- instead - the smallest use of the `passenger_ticket` event and `omsi.people.set_ticket`.
--
-- The ticket numbers are those of the stock Berlin ticket packs (TicketPacks\Berlin_94.otp):
-- 1 full rate, 2 short haul, 3 day ticket, 4 reduced rate, 5 short haul reduced rate.

local REDUCED = { [1] = 4, [2] = 5 }

omsi.on("passenger_ticket", function(p)
  local reduced = REDUCED[p.ticket]
  if p.kind ~= "buy" or not reduced then return end
  -- (the pack's ticket: its name, its price and the number the driver has to give with it)
  local ok, why = omsi.people.set_ticket(p.id, { ticket = reduced, tag = "reduced" })
  if not ok then omsi.log("passenger " .. p.id .. " keeps their ticket: " .. why) end
end)
