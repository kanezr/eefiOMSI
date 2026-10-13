/// The trembling that never stops while the bus runs: the grain of the tarmac under the
/// wheels, and the engine's own buzz coming up through the frame.
///
/// Both are small - a tenth of the wheel's lock at most, and a wheel with a real motor
/// spends most of its travel on parking resistance anyway - and both are fast enough to be
/// felt, which is why a wheel feels them as a buzz and not as a push. A constant-force
/// wheel can only be told one force at a time; a force that changes every few milliseconds
/// comes out of the motor as a tremble rather than as a movement of the wheel, which is
/// what the road is. A slow one is felt as nothing at all: the rim goes where it is told and
/// the hands go with it, so the rate matters more here than the size. And a force that lines
/// up with itself is felt as something solid being carried while one that does not is felt
/// as a loose surface, so most of the road is sines and only a little of it is noise.
///
/// It keeps a little state of its own: the distance the bus has rolled (the grain is read
/// along the road, so a bus standing still feels none of it and the same stretch of tarmac
/// shakes the same way twice) and the engine's angle (so its buzz is at the engine's own
/// rate whatever gear the bus is in, and a script's rpm that twitches does not reach the
/// wheel as a rattle).
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Micro {
    /// The distance the bus has rolled (m).
    pub(super) roll: f32,
    /// The engine's speed, with the twitches of the script that writes it taken out (rpm).
    pub(super) rpm: f32,
    /// Where each part of the engine's buzz stands (rad). Each is kept on its own because a
    /// phase reduced on its own rate is exact for that rate, and a phase shared between them
    /// is not: wrap the crank angle into a single turn, then take half or a quarter of it,
    /// and you land on a different half turn every time instead of carrying on.
    pub(super) crank: f32,
    pub(super) firing: f32,
    pub(super) mass: f32,
}

impl Micro {
    /// How much the wheel trembles this frame, of its full lock (-1..1). `road` and
    /// `engine` are the settings' strengths (0 = neither); the rest is what the bus is
    /// doing - `kmh`, the road's `street_cond` (0 dry, 1 wet, 2 snow), the engine's rpm and
    /// how hard it is working.
    pub(super) fn sample(&mut self, dt: f32, kmh: f32, street_cond: f32, rpm: f32, load: f32, road: f32, engine: f32) -> f32 {
        let dt = dt.clamp(0.0, 0.1);
        // The grain of the road: nothing at all at a standstill, all of it by 30 km/h, and
        // a wet or snowy road hums under the tyres more than dry asphalt does.
        let v = kmh.abs() / 3.6;
        let drive = (v / 8.5).min(1.0);
        let rough = 0.6 + 0.25 * street_cond.clamp(0.0, 2.0);
        self.roll += v * dt;
        // The road, read along the distance: mostly its ride - the shapes the whole bus is
        // being carried over - and only a seventh its grain. A wheel is told one force at a
        // time, and the difference the driver feels between a bus that feels heavy and one
        // whose wheel rattles is whether that force lines up with itself. Noise never does:
        // read along the road it gives every part of the spectrum a share, and a signal with
        // no shape to it is felt as a loose surface however fast it is felt. A ride made of
        // sines whose lengths do not fit together lines up and never repeats over a stretch
        // of road, and that is what reads as one solid weight being carried. The grain is
        // left in under it, small, because it is what says the road is made of something and
        // not polished. The three lengths are six metres, two and a quarter and nine
        // tenths: the deep ones are the mass, the last is what the tyre feels, and none of
        // them is so short that it comes back as a beat of its own before a wheel cannot be
        // told about it (a hundred times a second, so the 0.1 m grain is let go above 40 Hz,
        // where the 0.9 m one carries the surface on its own).
        // The game samples this signal once per frame. At 27 FPS, a 25 cm road
        // texture becomes a 33 Hz signal at only 30 km/h; sending those samples
        // aliases it into a slow wheel shake. Fade each spatial band before its
        // frequency reaches the frame's Nyquist limit (with headroom for the
        // harmonics of the interpolated road and a motor updated at up to 100 Hz).
        let shape = ride(self.roll / 6.0, v / 6.0, dt) * 0.34
            + ride(self.roll / 2.2 + 5.3, v / 2.2, dt) * 0.26
            + ride(self.roll / 0.9 + 11.9, v / 0.9, dt) * 0.4;
        let surface = grain(self.roll / 0.25) * 0.55 * sample_room(v / 0.25, dt)
            + grain(self.roll / 0.1 + 31.7) * 0.45 * sample_room(v / 0.1, dt);
        let tarmac = (shape * 0.85 + surface * 0.15) * drive * rough * 0.25 * road;
        // The engine's buzz, each part on the rate it really runs at: the crankshaft
        // turning, the firing pulses above it, and a low rumble under both standing for the
        // mass the frame works against. A wheel is told its force once a frame, so nothing
        // above about 28 Hz can be carried at all and each part is let go as it reaches for
        // it; above the revs where that has happened the crank and the pulses are both gone
        // and what is left is the low rumble.
        let turn = std::f32::consts::TAU * dt;
        self.rpm += (rpm.clamp(0.0, 4500.0) - self.rpm) * (dt / 0.08).min(1.0);
        let spin = |p: &mut f32, per_turn: f32| *p = (*p + self.rpm / per_turn * turn).rem_euclid(std::f32::consts::TAU);
        // a wheel is told its force once a frame, so nothing above about 28 Hz can be
        // carried at all and each part is let go over the last stretch before that
        let room = |f| sample_room(f, dt);
        spin(&mut self.crank, 60.0);
        spin(&mut self.firing, 30.0);
        spin(&mut self.mass, 240.0);
        let running = ((self.rpm - 150.0) / 400.0).clamp(0.0, 1.0);
        // an engine working against its mounts twists harder than one idling
        let mount = 0.7 + 0.6 * load.clamp(0.0, 1.0);
        let lump = 0.6 + 0.4 * (self.rpm / 2400.0).clamp(0.0, 1.0);
        let hum = running * mount * lump
            * (self.crank.sin() * room(self.rpm / 60.0) * 0.03
                + self.firing.sin() * room(self.rpm / 30.0) * 0.02
                + self.mass.sin() * (self.rpm / 2400.0).min(1.0) * 0.055)
            * engine;
        tarmac + hum
    }
}

/// The ride of the road under the wheels: smooth, always moving, and never the same twice.
///
/// Where `grain` reads the surface, this reads the shape. It is sines whose lengths do not
/// fit into one another, so it never repeats itself over a stretch of road and never jumps
/// either. Its sum is 1 at the very most, so it can be mixed by eye.
fn ride(x: f32, fundamental_hz: f32, dt: f32) -> f32 {
    let t = std::f32::consts::TAU * x;
    t.sin() * 0.55 * sample_room(fundamental_hz, dt)
        + (t * 1.7 + 1.3).sin() * 0.3 * sample_room(fundamental_hz * 1.7, dt)
        + (t * 2.9 + 4.2).sin() * 0.15 * sample_room(fundamental_hz * 2.9, dt)
}

/// Fade a vibration band to silence before it reaches the output's Nyquist
/// frequency. The 0.4 factor keeps room for harmonics and motor update limits.
pub(super) fn sample_room(frequency_hz: f32, dt: f32) -> f32 {
    let max_hz = (0.4 / dt.max(0.001)).min(28.0);
    let start = max_hz * 0.7;
    let x = ((frequency_hz - start) / (max_hz - start).max(0.001)).clamp(0.0, 1.0);
    1.0 - x * x * (3.0 - 2.0 * x)
}

/// Value noise along one line, -1..1. The grain of a road surface is not a sine wave, and
/// a wheel handed the same sine wave every few metres reads it as a whine from the gearbox
/// rather than as tarmac.
fn grain(x: f32) -> f32 {
    let i = x.floor();
    let f = x - i;
    let s = f * f * (3.0 - 2.0 * f); // the lattice's points, eased into one another
    let a = lattice(i as i32);
    let b = lattice(i as i32 + 1);
    (a + (b - a) * s) * 2.0 - 1.0
}

/// One point of the grain's lattice, hashed from its place in whole metres (so a long
/// drive does not walk the pattern off).
fn lattice(i: i32) -> f32 {
    let mut h = (i as u32).wrapping_mul(0x27d4_eb2d) ^ 0x9e37_79b1;
    h ^= h >> 15;
    h = h.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 13;
    (h >> 8) as f32 / 16_777_216.0
}