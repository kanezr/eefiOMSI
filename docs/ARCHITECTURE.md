# Architecture

openOMSI mirrors the unit structure of the original Delphi program so that every subsystem
has an obvious counterpart, but replaces its architecture where the original was limited:
64-bit, data loading and tessellation on a worker pool, a modern renderer, no global mutable
state (the aim; see "Known debt" below for where the code stands).

How it got here, round by round, is in [HISTORY.md](HISTORY.md); how a change that must not
change behaviour is checked is in [REFACTOR_CHECKS.md](REFACTOR_CHECKS.md).

| Original unit(s)                       | Crate / module                | Status |
|----------------------------------------|-------------------------------|--------|
| file readers (`TTextfile`)             | `omsi-cfg`                    | done, 100 % of stock content; content roots and mounted `.zip` archives through `omsi-cfg::vfs` |
| `mc_exprcalc` (scripts)                | `omsi-script`                 | done (compiler + VM), 245/245 stock script sets |
| `mc_o3dfiles`, `.x` meshes             | `omsi-o3d`                    | done incl. v4/v5 unscrambling and `.x` frame transforms |
| `mc_complobj`, `mc_MatlMan`            | `omsi-model`                  | parser done; materials drawn as Direct3D draws them (one-sided, envmap mask, bump map, `noZcheck`/`Zbias`, dynamic slots) |
| `mc_complMapObj`, `mc_splines`         | `omsi-scenery`                | parser done |
| `mc_mapclass`, `mc_terrain_2`, `mc_chrono`, `mc_fahrstrasse` | `omsi-map` | parser done; chrono folders applied (records patched by ID, lines taken off a date) |
| `mc_roadvehicle`, `mc_vehicle`, `mc_train`, `mc_passcabin`, `mc_sound` | `omsi-vehicle` | parser done |
| `mc_timetable`, `mc_station`           | `omsi-timetable`              | parser done; the runtime (duties, trips, layovers, departure boards) is `omsi-sim::timetable_run` (`ScheduleSim`), its buses put on the road by `omsi-app::schedule` |
| `mc_weather`, `mc_himmel`, `mc_font`, `mc_language`, `mc_money`, `mc_human`, `mc_driver`, `mc_situation`, `mc_input`, options | `omsi-content` | parsers done |
| `mc_texMan`                            | `omsi-texture`                | loading done incl. seasonal and `_LOW` variants; BC1-3 on the GPU (DXT as is, others compressed) and a texture budget |
| `mc_sound` runtime, DirectSound        | `omsi-audio` (cpal)           | mixer, WAV, loop/one-shot sounds with volcurves, conditions, triggers, 3D |
| `mc_font` text textures                | `omsi-sim::texttex`           | `[texttexture]` rendering from `.oft` fonts |
| tessellation, spline geometry          | `omsi-geometry`               | splines, terrain, placement done; the ground cut under flush surfaces and `[terrainhole]` |
| `mc_d3d_classes` (Direct3D 9)          | `omsi-render` (wgpu)          | the vanilla picture complete (materials, lights, night maps, reflections, mirrors, shadow cascades); `enhanced` is a physically based renderer of its own |
| `mc_Form_Main`, main loop              | `omsi-app`                    | offscreen + window, start menu, HUD, tile streaming; the original's dialogs are the launcher |
| vehicle runtime (`TRoadVehicleInst`)   | `omsi-sim`                    | scripts, animations, dynamic materials, keyboard/mouse input, IBIS typing, coupled parts |
| AI (`mc_path`, `mc_pathrule`), humans, physics (ODE) | `omsi-sim` | `traffic` + `ai_motion` (rules, light programs, following, passing), `human` + `crowd` (poses, queues, avoidance), `rigid` + `physics` (wheels, collisions) |
| Optionen and the start dialogs         | `omsi-launcher-ui` + `omsi-launcher-core` | the game's own window (wgpu, `omsi-ui`): profile, bus, map, duty, weather, settings, mods, running games (the Tauri launcher is gone since round 9) |
| - (no original counterpart)            | `omsi-net`, `omsi-lan`, `omsi-app::lan` | LAN play: UDP session, bit-packed vehicle states, chat |

## Crates

The workspace is layered; `scripts/arch-guard.sh` (rules in `scripts/arch-rules.txt`, run on
every pull request) keeps each crate off the layers above it, and `omsi-app` may only shrink.
Bottom up:

| Layer | Crates | What they hold |
|---|---|---|
| base | `omsi-cfg`, `omsi-ui` | text files, the virtual file system, settings, the `OMSI_*` flag registry, the panic guard; the immediate-mode UI toolkit |
| formats and content | `omsi-o3d`, `omsi-texture`, `omsi-model`, `omsi-map`, `omsi-scenery`, `omsi-vehicle`, `omsi-timetable`, `omsi-content`, `omsi-script`, `omsi-geometry`, `omsi-world` | files in, plain data out; the map-wide index and the placement of attached objects (`omsi-world`) |
| simulation | `omsi-sim` | vehicles (`vehicle/` by subsystem), scripts' host (`host/`), AI traffic, people, timetable, physics; no renderer, audio, texture decoder or network: pictures and HTML pages come through hooks the game registers (`image_decode`, `htmltex::set_backend`) |
| beside the simulation | `omsi-htmlengine`, `omsi-weather`, `omsi-career` | the `[htmltexture]` engine; the weather and season models; the driver's career and real-time mode |
| presentation | `omsi-render`, `omsi-audio`, `omsi-soundscape`, `omsi-photo`, `omsi-scene` | the renderer (`gpu/`, `resources/`, `scene/`, `lighting/`, `frame_plan`, shader parts); the mixer; ambience, scenery sounds and the radio; photo mode's lens and darkroom; the scene: loaded content turned into renderer data |
| devices and services | `omsi-input`, `omsi-xr`, `omsi-net`, `omsi-lan`, `omsi-platform`, `omsi-plugin`, `omsi-plugin-ui` | controllers, force feedback, head tracking, key names; OpenXR; the LAN transport and the game-side session; updater, Discord, the presence count, version stamp; plugin hosting and its UI |
| launcher | `omsi-launcher-core`, `omsi-launcher-ui` | the launcher's logic; its window and pages, talking to the game through `omsi_launcher_ui::host::Host` |
| app | `omsi-app` | the glue: `App` and its field groups, the frame (`frame_stages`: one declared list of stages for the window and one for the offscreen run, one runner), view sync, the adapters that hold the crates' state in `App` |

## Threading

* Tile loading (`.map` parse, terrain, spline tessellation, object type loading) runs on the
  rayon pool; GPU upload happens on the main thread. Object and spline *types* are shared
  through `Arc` caches guarded by mutexes.
* Tile *streaming* (`omsi-app::tiles::Streamer`) has a thread of its own: it decides which
  tiles belong around the camera and the player's bus, stages them on the rayon pool and
  hands them over, and the frame places and uploads only a little of one tile at a time
  (and unloads within the frame budget) so that streaming never shows as a stutter.
* Textures decode on worker threads through `TextureCache` and are compressed there
  (BC1/BC3) before they go up. A vehicle set read ahead for the timetable fleet is made on
  the GPU by the worker as well - wgpu takes device calls from any thread - so the drawing
  thread only puts it into the scene.
* People are posed and skinned in parallel (`par_iter_mut` over those due this frame, at a
  rate that falls with distance and with what the camera sees).
* The LAN socket lives in `omsi-net` on its own thread; the game's side of it is a stage of
  its own in `OMSI_PROFILE` ("lan": loading a joining player's bus used to be counted as
  the people's time).
* Ending the game from outside (`omsi-app::quit`): the signal handler only sets an atomic,
  a watcher thread wakes the event loop, and the ordinary shutdown path runs.
* AI vehicle scripts run in parallel (rayon `par_iter_mut` over the cars' `VehicleInstance`s
  after the sequential lane/obstacle planning of a frame); a lane grid (50 m cells,
  `Network::grid`) answers nearest-lane queries without scanning the whole network.
* The renderer rewrites only the per-draw entries of instances that changed since the last
  frame (`Scene::changed`); a full rebuild happens only when instances are added or the
  render origin moves. Together these took Spandau with traffic from ~34 to ~240 fps. It
  also culls in parallel, records the main pass as render bundles and finishes the command
  buffers on helper threads, and the enhanced sky is computed on a helper thread and taken
  in when it is ready.
* Planned: scenery object scripts in parallel, physics on its own step loop, audio on the
  audio thread.

## Memory

A 16 GB Mac shares its memory between the CPU and the GPU, so both count against the same
budget.

* Textures are kept compressed on the GPU (see FORMATS.md, "Textures on the GPU"): DXT
  files as blocks with their mip chains, other pictures compressed on the loader threads
  (`omsi-texture::bc`, a PCA/least-squares BC1/BC3 encoder, about 1 µs a block) when the
  result stays close. Pictures read on the thread that draws go up as RGBA and are swapped
  for their compressed copy by `World::apply_texture_upgrades` (the renderer rebuilds the
  bind groups of the materials using them).
* The timetable fleet: AI vehicle types are loaded without their CPU meshes
  (`VehicleType::load_ai`; a mesh is read again when its set goes to the GPU). A tour keeps
  one vehicle and paint scheme all day, chosen from the tour, so the vehicles of the
  departures of the next 25 minutes are known: they are read on the workers and uploaded
  one at a time ahead of their departure (`Schedule::fleet`), and a vehicle set nobody has
  drawn for 90 s and no coming departure wants leaves the GPU with the textures and meshes
  only it held (`World::trim_vehicle_sets`). An AI vehicle that goes gives its own
  instances, text and script textures and materials back to the free lists
  (`World::release_vehicle`); they were only hidden before.
* Vehicle sets read ahead are made on the GPU by the worker too (meshes and textures:
  the device takes calls from any thread, `omsi_render::prepare_mesh/prepare_texture`), so
  the upload on the thread that draws only puts them into the scene and makes the
  materials; materials made in one frame with the same textures and values share one bind
  group and uniform buffer (a C2 set: 965 materials, 60 ms before, about 4 ms now).
* `[matl_freetex]` pictures (destination and line pictures, 1024×640 on the C2) are shared
  by all vehicles showing them and compressed like other late loads; an AI vehicle more
  than 50 m from the camera shows its script textures (1024×512 cockpit and passenger
  displays) as a texel of their mean colour, and gets them back within 40 m.
* OMSI's `[texmemlimit]`: the scenery and vehicle textures have a budget (`texture_memory=`
  in MB, else an eighth of the machine's memory; `OMSI_TEXTURE_MEMORY` for tests). Once a
  second, while over it, the scenery textures that only tiles farther than 150 m use lose
  their finest mip level (a GPU copy into a smaller texture, farthest first, down to 64
  texels a side); with a tenth of the budget free, those within 400 m are read again on a
  worker and swapped back. Offscreen, `OMSI_BUDGET_FROM=x,y[,MB]` meets the budget from
  another place first (and raises it) to check the way back.
* Freed scene slots are handed out lowest first, and after unloads a free tail of the
  scene's arrays is cut off (`World::compact_slots`; a few live entries at the end keep the
  arrays long - moving live entries is not done).
* Sound clips nobody holds (no sound set, no voice) and nobody asked for in a minute leave
  the cache (`AudioEngine::trim_clips`; 140 MB of every vehicle that ever came into
  earshot before) and are read again in the background; the map index hands its 345 000
  object positions to `World::object_positions` instead of keeping a second copy.
* A tile's surface raster (which texels roads and plates cover, at which heights, and the
  terrain hole cutters) is kept in 16×16-texel blocks made when a surface first touches
  them, and a finished tile drops the lowest and drivable height layers of a block where
  they repeat the top surface (`TileSurface`): 5 MB a tile before, about 0.6 MB now.
* macOS's allocator keeps freed large blocks dirty for reuse (more than half a gigabyte
  after a few tile loads, and `malloc_zone_pressure_relief` does not return them); the game
  restarts itself at once with `MallocLargeCache=0` (exec, same pid; `OMSI_KEEP_ALLOCATOR=1`
  skips it). The small-allocation zones are asked to give their free pages back on a
  thread of its own after tile loads and unloads and after the fleet shrinks.
* `OMSI_PROFILE` logs, every ten seconds, the GPU memory by kind (scenery textures by
  format, vehicle textures, tiles' own, meshes, draw data) and the CPU side (object type
  meshes, staged tiles, surface rasters, wheel grids); `OMSI_DEBUG_TEXTURES` lists every
  vehicle texture uploaded and all textures by format and size.
* Ahlheim V5 main station with traffic 30, passengers and the timetable (window, 60 s):
  peak footprint (`/usr/bin/time -l`) 8.75 GB before, 1.93 GB after, flat over the minute
  instead of growing by 700 MB; a flight over Ahlheim 8.54 → 2.05 GB (1.38 instead of
  6.51 GB after flying back); Spandau with the EN92, traffic 30, passengers and the
  timetable 3.45 → 1.20 GB. Frames over 50 ms: 2 → 0, 3 → 0 and 0 → 0; average frame rate
  51.7 → 57.6, 119 → 123 and 121 → 124 fps (1280×720 window, 1x MSAA).

## Coordinate frames

* World: x east, y north, z up, metres; headings in degrees clockwise from north.
* `.cfg` files (cameras, positions, animations): x right, y forward, z up.
* Mesh files (`.o3d`/`.x`): Direct3D frame, converted on load (`omsi-geometry::mesh_from_o3d`).
  A DirectX `.x` `FrameTransformMatrix` is row-major for row vectors, which read column by
  column is already glam's column-vector matrix (no transpose); normals go by the inverse
  transpose.
* Vehicle frame = the `.cfg` frame (x right, y forward, z up), origin at the model's z = 0:
  the plane the tyres touch with the springs **unloaded**, so a standing bus's origin is
  10-16 cm above the road. `coll_pos_*` is in that frame, at bumper height.
* Angles: `Wheel_Rotation_*` and `Axle_Steering_*` are radians, positive to the right;
  `articulation_<n>_alpha` is the joint's yaw in clockwise degrees (`beta` its pitch), `n`
  the coupled part's position in the train.
* Map splines store x, height, y; objects store x, y, z. See `docs/FORMATS.md`.


## Start guard

A release that breaks the start must not strand anyone (it happened: the updater lives in the
launcher, and a launcher that never drew its window never updated). `crates/omsi-guard` runs
before anything that could break and needs no graphics card, window or game state (the arch
rules forbid it the renderer, the UI, the simulation, the scene, the audio and the launcher UI):

* `main` asks it first: `--self-test` (the updater runs a new program with it before installing
  it - a build that cannot be loaded is refused there), `--guard-watch` (the watcher), safe
  mode's switches, then `omsi_guard::start(Launcher)` for a launcher start, which starts the
  watcher - the same program again, without a window - or heals right away when this version
  already failed here.
* The watcher waits for the launcher's first frames (`omsi_guard::healthy`, called by the
  launcher at its first presented frame and by a game session after 60 frames), its end, or
  `OMSI_GUARD_DEADLINE` (120 s, then it ends it). A failed start of a version that never ran
  here, or two in a row, is healed (`rescue`): a newer release, else the version before
  (`omsi_platform::recovery::rollback` - the updater keeps the replaced files until the new
  version has started well), else safe mode, else a message. Meanwhile it reads the project's
  list of broken releases (`release/health.json`); a listed version keeps its way back and goes
  back at the next start.
* `guard/starts.json` (the journal) also notes the game sessions the launcher starts and starts
  no watcher saw: a start that never reached its first frames and whose process is gone failed.
  `guard/bad-versions` lists what failed here; the updater offers none of them again.
* Proof: `scripts/guard-e2e.sh <old build> <new build>` runs real release builds in a throwaway
  installation against fake releases and versions broken on purpose (`OMSI_GUARD_TEST_BREAK`).

## Target layering

Where the code is going (new code follows it, refactors move old code towards it):

1. **Parsers** (`omsi-cfg`, `omsi-script`, `omsi-o3d`, `omsi-model`, `omsi-scenery`, `omsi-map`,
   `omsi-vehicle`, `omsi-timetable`, `omsi-content`): files in, plain data out. No GPU, no
   clock, no global state; each one testable on byte buffers.
2. **Simulation** (`omsi-sim`: vehicles, `ai_traffic` (`TrafficSim`), `people` (`PeopleSim`),
   `timetable_run` (`ScheduleSim`), physics, scripts): advances
   the world by a time step. Knows nothing of the GPU or the window: it can run headless, in
   tests and on a server, and two runs with the same seed and inputs give the same states.
3. **View sync**: the one place that turns simulation state into renderer instances
   (positions, animation matrices, materials, script textures, lights) and tells the renderer
   what changed. Simulation code does not call the renderer, and the renderer does not read
   simulation types.
4. **Render** (`omsi-render`, `omsi-texture`, `omsi-geometry` for the meshes): draws what it is
   given. It owns the GPU and nothing of the game.

`omsi-app` is the frame pipeline that runs these as **phases** in a fixed order - input,
simulation step, view sync, render, present - for both the window and the offscreen run,
so `--offscreen` exercises the same code as the game. A phase is a function with explicit
inputs and outputs rather than a stretch of a long event handler.

**Debug flags** (`OMSI_DEBUG_*`, `OMSI_NO_*`, the checks behind environment variables) are
read once, through one registry that names each flag, its meaning and its type, instead of
`env::var` calls scattered through the code, so the list in `docs/USER_GUIDE.md` can be
checked against it.

## Known debt

* **What is already in place.** The frame is a declared list of stages run by one runner
  (`omsi-app/src/frame_stages.rs`: `app_events::WINDOW_FRAME` for the window,
  `offscreen::OFFSCREEN_STEP` for the offscreen run, each stage naming the field groups it
  reads and writes and the values it hands on; tests pin both orders and the three places
  where the two paths knowingly differ; `OMSI_FRAME_ORDER=1` logs them); the AI traffic and the people simulate in
  `omsi-sim`, with `omsi-app::traffic::Traffic` and `omsi-app::humans::Humans` as thin
  wrappers (the simulation, with the cars' sounds) that dereference to them; every `OMSI_*` switch goes through
  `omsi_cfg::flags` (`docs/DEBUG_FLAGS.md`); `App`'s state is grouped by subsystem
  (`omsi-app/src/app/groups.rs`).

* **The timetable** is `omsi-sim::timetable_run`: the trip times, the IBIS, the player's
  duty, and `ScheduleSim` (`timetable_run/sim/`) - the day's departures and which are due,
  queued, waiting for their tiles or awaiting their tour's bus, the tours' vehicles
  (`choose`), the routes on the lanes and where on its route a due bus is
  (`place_departure`), the routes carried on as tiles load, the tours, the duty taken
  from one and the departure boards. It reads the map through `TimetableWorld` (omsi-app's
  `World` implements it) and the traffic as `TrafficSim`. `omsi-app::schedule::Schedule`
  holds it (`Deref` to `ScheduleSim`) as the GPU adapter: `fleet` reads and uploads the
  vehicle sets of the next departures, `dispatch` and `spawn` put the buses on the road
  (`Traffic::spawn_bus`, `set_trailers`, `attach_cars`, `turn_train`), hand a tour's bus
  its next trip, and take buses off, each telling `ScheduleSim` what became of the
  departure. Still in the adapter: the bus put on the road itself (its trailers and
  cars, where it starts, whether it may appear there), which needs the app's `Traffic`
  (`trailer_chain`, `may_appear`, `blocked`) besides the GPU.

* **The view sync phase** (`omsi-app/src/view_sync/`): the code that turns the traffic's
  and the people's state into renderer instances is in one module - `traffic.rs` (the AI
  vehicles' renders, their drivers, the traffic lamps), `people.rs` (the people's meshes,
  posing and skinning, the coins and ticket blocks) - with one entry point,
  `view_sync::sync(ViewSync { traffic, people }, sim_view, world, renderer, scene)`,
  traffic first. Its state, `view_sync::SimView { traffic: TrafficView, people:
  PeopleView }` (the cars' renders and drivers; the people's meshes, GPU materials, posing
  and ticket blocks), is held apart from the simulation: by the window in
  `GfxState::sim_view`, by the offscreen run in its own struct. Each part starts afresh
  with the simulation it shows (the `Traffic` put in place, `Humans::new`).
  It runs where each part was synced before, so that the renderer sees the same calls in
  the same order:
  * the window: the traffic at the end of `frame_traffic` (after its step and sound,
    before the player and the people move: the cars that parked leave the traffic's list
    there, and the people's step reads it), the people at the end of `frame_people` (after
    their step and the coins handed out);
  * the offscreen run: the traffic after each population (and before an
    `OMSI_POPULATION_SHOTS` picture), the people every step while `OMSI_TRACE_PAX` traces,
    both before each snapshot (the traffic, the player's pose, then the people from the
    snapshot's camera) and in the closing reports.

  Not yet in the phase: renders made or let go inside the wrappers' own calls, at the
  moment the simulation needs them - a car's when the traffic puts it on the road or takes
  it off (population, timetable departures, trains, the LAN mirror, its step's retired
  cars), and the people who appeared or went at the end of every `Humans` call that made
  them (`BodyOp`s replayed by `Humans::show_bodies`). Those calls take the part of the
  `SimView` they change (`&mut TrafficView`, `&mut PeopleView`) with the renderer, and
  the timetable's and the LAN world's steps that make them pass it through. The traffic's sync still hands the AI vehicles `Envir_Brightness`; the cars that
  finished parking are taken off by the simulation itself (`TrafficSim::take_parked`), the
  sync only puts their parked objects back. The player's and the AI buses' per-frame sync
  (materials, lamps, script and text textures, glass cracks, dents) is in the phase too:
  `view_sync/vehicle.rs` and `vehicle_damage.rs`, and the AI cars' sounds in
  `view_sync/traffic_audio.rs`.

* **Global mutable state.** The introduction's "no global mutable state" is still not
  quite true, but what is left no longer carries a session's state into the next one. The
  game state that lived in statics (a second drive started from the phone's launcher runs
  in the same process, and the window, the offscreen run and the launcher's showroom share
  it) moved into its owners: the physical weather model and the classic sky's cloud type
  into `weather_setup::SkyState` (kept by the session, the offscreen run and the showroom),
  the duty start's opened world and the real-time start into `Args`, the map lights near
  the camera, the fading spotlight cookies, the spline-check ends and the season-mix counts
  into `World`, the passengers' waiting voice lines, the photo mode's settings and the
  settings window's flags into `App`'s groups, the `/admin` password into `LanGame`, the
  drive's extremes into `Offscreen`, and which corona pictures a renderer holds into the
  `Renderer` (`pictures_taken`). Left: in `omsi-app` 20 module-level statics with interior
  mutability, 35 more inside functions and `thread_local!` blocks (9 of those); in
  `omsi-scene` 14 and 15, no `thread_local!` (count them with
  `grep -rnE '^\s*(pub(\([a-z:]+\))? )?static ' crates/omsi-app/src | grep -E 'Mutex|Atomic|OnceLock|RwLock|Cell|Lazy|Once\b'`
  and `grep -rn 'thread_local!'`). They are install-once hooks, platform
  handles and process resources (the signal handler, the tilt sensor, the translation
  worker, thread pools, the LAN gateway and mods root), memos of file contents and
  environment switches, rate limits of log lines and refreshes, the picture-file numbering
  that is the same for every renderer, and a handful of settings and the session's weather
  broadcast to loading code without game state at hand (`scene::host`, `SNOW_WEATHER`,
  `rain::QUALITY`, `startup::{CLOUDS, SOUND_AI, ENHANCED_PLUS}`, the lamps' cone weather),
  set again at every session start or frame. Those last are the next to go, once the
  vehicle and world loading take them as arguments.
* **Very long functions and files.** `scripts/size-budget.sh` (run on every pull request)
  keeps them from growing; `scripts/size-budget.txt` lists those over the limits
  (1500 lines a file, 300 a function) with their recorded sizes. The baseline only goes down.
