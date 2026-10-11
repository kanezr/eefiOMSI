//! Roundabouts found in the path network: a ring is a loop of street lanes, short enough to
//! be driven round in one go, that turns once round in the direction the traffic goes round
//! a roundabout (anticlockwise where the map drives on the right, clockwise on the left).
//! The maps do not mark their rings, and Omsi.exe has no ring of its own (it decides a ring
//! like any junction, first come first served); the cars here let the ring go first where
//! the map sets no priorities (`Network::must_yield`).

use super::{wrap_deg, LaneKind, Network};

/// A ring is at most this long all round (m): the largest roundabouts of the stock maps
/// are some 60 m across.
pub const RING_MAX: f32 = 190.0;
/// The lanes looked at from one start before it is given up (a junction's short paths
/// branch a lot).
const SEARCH_MAX: usize = 2000;
/// Heading change (degrees) under which a stretch of the loop counts as straight, and the
/// share of the loop's length the longest straight stretch may have: a ring bends all the
/// way round, a block of streets driven round by its corners is mostly straight.
const STRAIGHT_TURN: f32 = 15.0;
const STRAIGHT_SHARE: f32 = 0.15;

/// Heading change along lane `l` from its start to its end (degrees, + to the right).
fn lane_turn(net: &Network, l: usize) -> f32 {
    net.lanes[l].headings.windows(2).map(|w| wrap_deg(w[1] - w[0])).sum()
}

/// The loop of lanes `cycle` turns once round the way a roundabout's traffic does, and
/// bends all the way round.
fn is_ring(net: &Network, cycle: &[usize]) -> bool {
    // heading as a continuous function of the distance round the loop
    let mut samples: Vec<(f32, f32)> = Vec::new();
    let (mut along, mut heading) = (0.0f32, 0.0f32);
    for (k, &l) in cycle.iter().enumerate() {
        let lane = &net.lanes[l];
        if k > 0 {
            heading += wrap_deg(lane.start_heading() - net.lanes[cycle[k - 1]].end_heading());
        }
        for (p, w) in lane.headings.windows(2).enumerate() {
            samples.push((along + lane.dist[p], heading));
            heading += wrap_deg(w[1] - w[0]);
        }
        along += lane.length();
    }
    heading += wrap_deg(net.lanes[cycle[0]].start_heading() - net.lanes[*cycle.last().unwrap()].end_heading());
    // round once, anticlockwise (headings count clockwise from north) on the right
    let want = if net.left_hand { 360.0 } else { -360.0 };
    if (heading - want).abs() > 45.0 || samples.is_empty() {
        return false;
    }
    let total = along.max(1.0);
    // (twice round, for the stretches over the loop's start)
    let n = samples.len();
    let twice: Vec<(f32, f32)> = samples.iter().copied().chain(samples.iter().map(|&(d, h)| (d + total, h + heading))).collect();
    let mut longest = 0.0f32;
    for (a, &(da, ha)) in samples.iter().enumerate() {
        let mut end = da;
        for &(db, hb) in &twice[a..a + n] {
            if (hb - ha).abs() > STRAIGHT_TURN {
                break;
            }
            end = db;
        }
        longest = longest.max(end - da);
    }
    longest < STRAIGHT_SHARE * total
}

/// The shortest loop of street lanes from `start` back to it within `RING_MAX` (lanes in
/// order, `start` first), if any.
fn shortest_loop(net: &Network, start: usize) -> Option<Vec<usize>> {
    // breadth first by length: (length so far, path)
    let mut open: Vec<(f32, Vec<usize>)> = vec![(net.lanes[start].length(), vec![start])];
    let mut seen = 0usize;
    let mut best: Option<(f32, Vec<usize>)> = None;
    while let Some(k) = open
        .iter()
        .enumerate()
        .min_by(|a, b| a.1 .0.total_cmp(&b.1 .0))
        .map(|(k, _)| k)
    {
        let (len, path) = open.swap_remove(k);
        if best.as_ref().is_some_and(|b| b.0 <= len) {
            break;
        }
        seen += 1;
        if seen > SEARCH_MAX {
            break;
        }
        let last = *path.last().unwrap();
        for &n in &net.lanes[last].next {
            let lane = &net.lanes[n];
            if lane.kind != LaneKind::Street {
                continue;
            }
            if n == start {
                if best.as_ref().is_none_or(|b| len < b.0) {
                    best = Some((len, path.clone()));
                }
                continue;
            }
            let l = len + lane.length();
            if l > RING_MAX || path.contains(&n) {
                continue;
            }
            let mut p = path.clone();
            p.push(n);
            open.push((l, p));
        }
    }
    best.map(|b| b.1)
}

/// Mark the ring lanes among the lanes from index `first` on (`Lane::ring`); returns how
/// many rings were found.
pub fn find_rings(net: &mut Network, first: usize) -> usize {
    let mut found = 0;
    for start in first..net.lanes.len() {
        let lane = &net.lanes[start];
        if lane.ring || lane.kind != LaneKind::Street || lane.length() > RING_MAX || lane.next.is_empty() {
            continue;
        }
        // (a lane that turns the wrong way already is on no ring)
        let t = lane_turn(net, start);
        if (net.left_hand && t < -45.0) || (!net.left_hand && t > 45.0) {
            continue;
        }
        let Some(cycle) = shortest_loop(net, start) else { continue };
        if !is_ring(net, &cycle) {
            continue;
        }
        if cycle.iter().all(|&l| net.lanes[l].ring) {
            continue;
        }
        for &l in &cycle {
            net.lanes[l].ring = true;
        }
        found += 1;
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traffic::LaneBuilder;
    use glam::DVec3;

    /// A ring of radius `r` cut into `n` arcs, driven anticlockwise (`ccw`) or clockwise.
    pub fn ring(r: f64, n: usize, ccw: bool) -> Vec<crate::traffic::Lane> {
        let step = 360.0 / n as f64;
        let arc = r * step.to_radians();
        (0..n)
            .map(|k| {
                // start on the circle round the origin at angle a (from north, clockwise)
                let a = if ccw { -(k as f64) * step } else { k as f64 * step };
                let p = DVec3::new(r * a.to_radians().sin(), r * a.to_radians().cos(), 0.0);
                // tangent: anticlockwise round the centre heads 90° left of the radius
                let heading = if ccw { a - 90.0 } else { a + 90.0 };
                let radius = if ccw { -r } else { r };
                LaneBuilder::arc(p, heading, arc, radius, 0.0, LaneKind::Street, 3.0)
            })
            .collect()
    }

    fn network(lanes: Vec<crate::traffic::Lane>, left_hand: bool) -> Network {
        let mut net = Network { lanes, left_hand, ..Default::default() };
        net.link(1.5);
        net
    }

    #[test]
    fn an_anticlockwise_loop_is_a_ring_on_the_right() {
        let net = network(ring(15.0, 4, true), false);
        assert!(net.lanes.iter().all(|l| l.ring));
    }

    #[test]
    fn a_loop_driven_the_wrong_way_round_is_no_ring() {
        let net = network(ring(15.0, 4, false), false);
        assert!(net.lanes.iter().all(|l| !l.ring));
        // mirrored on a map that drives on the left
        let net = network(ring(15.0, 4, false), true);
        assert!(net.lanes.iter().all(|l| l.ring));
        let net = network(ring(15.0, 4, true), true);
        assert!(net.lanes.iter().all(|l| !l.ring));
    }

    #[test]
    fn a_big_loop_and_a_block_of_streets_are_no_rings() {
        // 60 m across is still a ring, 100 m across is a ring road, not a roundabout
        assert!(network(ring(28.0, 6, true), false).lanes.iter().all(|l| l.ring));
        assert!(network(ring(50.0, 6, true), false).lanes.iter().all(|l| !l.ring));
        // a 40 m block driven round by its four left turns
        let mut lanes = Vec::new();
        let (mut at, mut h) = (DVec3::ZERO, 0.0f64);
        for _ in 0..4 {
            let side = LaneBuilder::arc(at, h, 34.0, 0.0, 0.0, LaneKind::Street, 3.0);
            let corner = LaneBuilder::arc(side.end(), h, 3.0 * std::f64::consts::FRAC_PI_2, -3.0, 0.0, LaneKind::Street, 3.0);
            at = corner.end();
            h -= 90.0;
            lanes.push(side);
            lanes.push(corner);
        }
        let net = network(lanes, false);
        assert!(net.lanes.iter().all(|l| !l.ring));
    }
}
