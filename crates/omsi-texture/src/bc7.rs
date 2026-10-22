use rayon::prelude::*;

const WEIGHTS: [f32; 4] = [0.299, 0.587, 0.114, 16.0];

const W2: [u32; 4] = [0, 21, 43, 64];
const W3: [u32; 8] = [0, 9, 18, 27, 37, 46, 55, 64];
const W4: [u32; 16] = [0, 4, 9, 13, 17, 21, 26, 30, 34, 38, 43, 47, 51, 55, 60, 64];

const P2: [u16; 64] = [
    0xcccc, 0x8888, 0xeeee, 0xecc8, 0xc880, 0xfeec, 0xfec8, 0xec80, 0xc800, 0xffec, 0xfe80, 0xe800,
    0xffe8, 0xff00, 0xfff0, 0xf000, 0xf710, 0x008e, 0x7100, 0x08ce, 0x008c, 0x7310, 0x3100, 0x8cce,
    0x088c, 0x3110, 0x6666, 0x366c, 0x17e8, 0x0ff0, 0x718e, 0x399c, 0xaaaa, 0xf0f0, 0x5a5a, 0x33cc,
    0x3c3c, 0x55aa, 0x9696, 0xa55a, 0x73ce, 0x13c8, 0x324c, 0x3bdc, 0x6996, 0xc33c, 0x9966, 0x0660,
    0x0272, 0x04e4, 0x4e40, 0x2720, 0xc936, 0x936c, 0x39c6, 0x639c, 0x9336, 0x9cc6, 0x817e, 0xe718,
    0xccf0, 0x0fcc, 0x7744, 0xee22,
];

const P3: [u32; 64] = [
    0xaa685050, 0x6a5a5040, 0x5a5a4200, 0x5450a0a8, 0xa5a50000, 0xa0a05050, 0x5555a0a0, 0x5a5a5050,
    0xaa550000, 0xaa555500, 0xaaaa5500, 0x90909090, 0x94949494, 0xa4a4a4a4, 0xa9a59450, 0x2a0a4250,
    0xa5945040, 0x0a425054, 0xa5a5a500, 0x55a0a0a0, 0xa8a85454, 0x6a6a4040, 0xa4a45000, 0x1a1a0500,
    0x0050a4a4, 0xaaa59090, 0x14696914, 0x69691400, 0xa08585a0, 0xaa821414, 0x50a4a450, 0x6a5a0200,
    0xa9a58000, 0x5090a0a8, 0xa8a09050, 0x24242424, 0x00aa5500, 0x24924924, 0x24499224, 0x50a50a50,
    0x500aa550, 0xaaaa4444, 0x66660000, 0xa5a0a5a0, 0x50a050a0, 0x69286928, 0x44aaaa44, 0x66666600,
    0xaa444444, 0x54a854a8, 0x95809580, 0x96969600, 0xa85454a8, 0x80959580, 0xaa141414, 0x96960000,
    0xaaaa1414, 0xa05050a0, 0xa0a5a5a0, 0x96000000, 0x40804080, 0xa9a8a9a8, 0xaaaaaa44, 0x2a4a5254,
];

const A2: [u8; 64] = [
    15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 2, 8, 2, 2, 8, 8, 15, 2, 8,
    2, 2, 8, 8, 2, 2, 15, 15, 6, 8, 2, 8, 15, 15, 2, 8, 2, 2, 2, 15, 15, 6, 6, 2, 6, 8, 15, 15, 2,
    2, 15, 15, 15, 15, 15, 2, 2, 15,
];

const A3: [[u8; 64]; 2] = [
    [
        3, 3, 15, 15, 8, 3, 15, 15, 8, 8, 6, 6, 6, 5, 3, 3, 3, 3, 8, 15, 3, 3, 6, 10, 5, 8, 8, 6,
        8, 5, 15, 15, 8, 15, 3, 5, 6, 10, 8, 15, 15, 3, 15, 5, 15, 15, 15, 15, 3, 15, 5, 5, 5, 8,
        5, 10, 5, 10, 8, 13, 15, 12, 3, 3,
    ],
    [
        15, 8, 8, 3, 15, 15, 3, 8, 15, 15, 15, 15, 15, 15, 15, 8, 15, 8, 15, 3, 15, 8, 15, 8, 3,
        15, 6, 10, 15, 15, 10, 8, 15, 3, 15, 10, 10, 8, 9, 10, 6, 15, 8, 15, 3, 6, 6, 8, 15, 3, 15,
        15, 15, 15, 15, 15, 15, 15, 15, 15, 3, 15, 15, 8,
    ],
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum PBit {
    None,
    Shared,
    Unique,
}

struct ModeInfo {
    subsets: usize,
    part_bits: u32,
    cb: u32,
    ab: u32,
    pbit: PBit,
    ib: u32,
    ib2: u32,
}

const MODES: [ModeInfo; 8] = [
    ModeInfo {
        subsets: 3,
        part_bits: 4,
        cb: 4,
        ab: 0,
        pbit: PBit::Unique,
        ib: 3,
        ib2: 0,
    },
    ModeInfo {
        subsets: 2,
        part_bits: 6,
        cb: 6,
        ab: 0,
        pbit: PBit::Shared,
        ib: 3,
        ib2: 0,
    },
    ModeInfo {
        subsets: 3,
        part_bits: 6,
        cb: 5,
        ab: 0,
        pbit: PBit::None,
        ib: 2,
        ib2: 0,
    },
    ModeInfo {
        subsets: 2,
        part_bits: 6,
        cb: 7,
        ab: 0,
        pbit: PBit::Unique,
        ib: 2,
        ib2: 0,
    },
    ModeInfo {
        subsets: 1,
        part_bits: 0,
        cb: 5,
        ab: 6,
        pbit: PBit::None,
        ib: 2,
        ib2: 3,
    },
    ModeInfo {
        subsets: 1,
        part_bits: 0,
        cb: 7,
        ab: 8,
        pbit: PBit::None,
        ib: 2,
        ib2: 2,
    },
    ModeInfo {
        subsets: 1,
        part_bits: 0,
        cb: 7,
        ab: 7,
        pbit: PBit::Unique,
        ib: 4,
        ib2: 0,
    },
    ModeInfo {
        subsets: 2,
        part_bits: 6,
        cb: 5,
        ab: 5,
        pbit: PBit::Unique,
        ib: 2,
        ib2: 0,
    },
];

const SWATCH_GOOD: f32 = 64.0;
const SWATCH_LIMIT: f32 = 1600.0;
const ALPHA_RETRY: i32 = 16;

#[inline]
fn interp_weights(bits: u32) -> &'static [u32] {
    match bits {
        2 => &W2,
        3 => &W3,
        _ => &W4,
    }
}

#[inline]
fn subset_of(subsets: usize, part: usize, i: usize) -> usize {
    match subsets {
        2 => (P2[part] >> i) as usize & 1,
        3 => (P3[part] >> (2 * i)) as usize & 3,
        _ => 0,
    }
}

fn anchors(subsets: usize, part: usize) -> [usize; 3] {
    match subsets {
        2 => [0, A2[part] as usize, 0],
        3 => [0, A3[0][part] as usize, A3[1][part] as usize],
        _ => [0; 3],
    }
}

#[inline]
fn expand(code: u32, bits: u32) -> u32 {
    let s = code << (8 - bits);
    s | (s >> bits)
}

#[inline]
fn end_value(q: u8, p: u8, bits: u32, pbit: PBit) -> u32 {
    if pbit == PBit::None {
        expand(q as u32, bits)
    } else {
        expand(((q as u32) << 1) | p as u32, bits + 1)
    }
}

fn quantize(v: f32, p: u8, bits: u32, pbit: PBit) -> u8 {
    let max = (1u32 << bits) - 1;
    let guess = if pbit == PBit::None {
        v * max as f32 / 255.0
    } else {
        (v * ((max << 1) | 1) as f32 / 255.0 - p as f32) * 0.5
    };
    let g = guess.round().clamp(0.0, max as f32) as u32;
    let mut best = (f32::MAX, g);
    for q in g.saturating_sub(1)..=(g + 1).min(max) {
        let d = (end_value(q as u8, p, bits, pbit) as f32 - v).abs();
        if d < best.0 {
            best = (d, q);
        }
    }
    best.1 as u8
}

#[derive(Clone, Copy)]
struct Spec {
    nch: usize,
    bits: [u32; 4],
    pbit: PBit,
    ib: u32,
    w: [f32; 4],
    p_one: bool,
    polish: bool,
}

impl Spec {
    fn combos(&self) -> &'static [[u8; 2]] {
        match (self.pbit, self.p_one) {
            (PBit::None, _) => &[[0, 0]],
            (_, true) => &[[1, 1]],
            (PBit::Shared, false) => &[[0, 0], [1, 1]],
            (PBit::Unique, false) => &[[0, 0], [0, 1], [1, 0], [1, 1]],
        }
    }
}

#[derive(Clone, Copy)]
struct Fit {
    q: [[u8; 4]; 2],
    p: [u8; 2],
    idx: [u8; 16],
    dec: [[u8; 4]; 16],
    err: f32,
}

fn evaluate(px: &[[f32; 4]], s: &Spec, q: [[u8; 4]; 2], p: [u8; 2]) -> Fit {
    let ws = interp_weights(s.ib);
    let n = ws.len();
    let mut pal = [[0u8; 4]; 16];
    let mut palf = [[0f32; 4]; 16];
    let mut w = [0f32; 4];
    w[..s.nch].copy_from_slice(&s.w[..s.nch]);
    for c in 0..s.nch {
        let e0 = end_value(q[0][c], p[0], s.bits[c], s.pbit);
        let e1 = end_value(q[1][c], p[1], s.bits[c], s.pbit);
        for (k, &wk) in ws.iter().enumerate() {
            let v = ((64 - wk) * e0 + wk * e1 + 32) >> 6;
            pal[k][c] = v as u8;
            palf[k][c] = v as f32;
        }
    }
    let mut dir = [0f32; 4];
    let mut dd = 0.0;
    for c in 0..4 {
        dir[c] = (palf[n - 1][c] - palf[0][c]) * w[c];
        dd += dir[c] * (palf[n - 1][c] - palf[0][c]);
    }
    let scale = if dd > 0.0 { (n - 1) as f32 / dd } else { 0.0 };
    let mut f = Fit {
        q,
        p,
        idx: [0; 16],
        dec: [[0; 4]; 16],
        err: 0.0,
    };
    for (i, x) in px.iter().enumerate() {
        let (lo, hi) = if n >= 8 {
            let t: f32 = (0..4).map(|c| (x[c] - palf[0][c]) * dir[c]).sum::<f32>() * scale;
            let k = t.round().clamp(0.0, (n - 1) as f32) as usize;
            (k.saturating_sub(1), (k + 1).min(n - 1))
        } else {
            (0, n - 1)
        };
        let mut best = (f32::MAX, 0usize);
        for (k, e) in palf.iter().enumerate().take(hi + 1).skip(lo) {
            let mut d = 0.0;
            for c in 0..4 {
                let t = x[c] - e[c];
                d += t * t * w[c];
            }
            if d < best.0 {
                best = (d, k);
            }
        }
        f.idx[i] = best.1 as u8;
        f.dec[i] = pal[best.1];
        f.err += best.0;
    }
    f
}

fn try_ends(px: &[[f32; 4]], s: &Spec, e0: [f32; 4], e1: [f32; 4]) -> Fit {
    let mut best: Option<Fit> = None;
    for &pp in s.combos() {
        let mut q = [[0u8; 4]; 2];
        for c in 0..s.nch {
            q[0][c] = quantize(e0[c], pp[0], s.bits[c], s.pbit);
            q[1][c] = quantize(e1[c], pp[1], s.bits[c], s.pbit);
        }
        let f = evaluate(px, s, q, pp);
        if best.is_none_or(|b| f.err < b.err) {
            best = Some(f);
        }
    }
    best.unwrap()
}

fn single(px: &[[f32; 4]], s: &Spec) -> Fit {
    let ws = interp_weights(s.ib);
    let v = px[0];
    let mut best = (f32::MAX, [[0u8; 4]; 2], [0u8; 2]);
    for &pp in s.combos() {
        for &w in &ws[..=ws.len() / 2] {
            let mut q = [[0u8; 4]; 2];
            let mut err = 0.0;
            for c in 0..s.nch {
                let (bits, max) = (s.bits[c], (1i32 << s.bits[c]) - 1);
                let g0 = quantize(v[c], pp[0], bits, s.pbit) as i32;
                let g1 = quantize(v[c], pp[1], bits, s.pbit) as i32;
                let mut cb = (f32::MAX, 0u8, 0u8);
                for q0 in (g0 - 2).max(0)..=(g0 + 2).min(max) {
                    let e0 = end_value(q0 as u8, pp[0], bits, s.pbit);
                    for q1 in (g1 - 2).max(0)..=(g1 + 2).min(max) {
                        let e1 = end_value(q1 as u8, pp[1], bits, s.pbit);
                        let d = ((((64 - w) * e0 + w * e1 + 32) >> 6) as f32 - v[c]).abs();
                        if d < cb.0 {
                            cb = (d, q0 as u8, q1 as u8);
                        }
                    }
                }
                q[0][c] = cb.1;
                q[1][c] = cb.2;
                err += cb.0 * cb.0 * s.w[c];
            }
            if err < best.0 {
                best = (err, q, pp);
            }
        }
    }
    evaluate(px, s, best.1, best.2)
}

fn moments(px: &[[f32; 4]], nch: usize, sw: &[f32; 4]) -> ([f32; 4], [[f32; 4]; 4]) {
    let n = px.len() as f32;
    let mut mean = [0f32; 4];
    for x in px {
        for c in 0..nch {
            mean[c] += x[c];
        }
    }
    for m in &mut mean {
        *m /= n;
    }
    let mut cov = [[0f32; 4]; 4];
    for x in px {
        let mut d = [0f32; 4];
        for c in 0..nch {
            d[c] = (x[c] - mean[c]) * sw[c];
        }
        for a in 0..nch {
            for b in 0..nch {
                cov[a][b] += d[a] * d[b];
            }
        }
    }
    (mean, cov)
}

fn principal_axis(cov: &[[f32; 4]; 4], nch: usize) -> [f32; 4] {
    let mut k = 0;
    for c in 1..nch {
        if cov[c][c] > cov[k][k] {
            k = c;
        }
    }
    let mut v = [0f32; 4];
    v[..nch].copy_from_slice(&cov[k][..nch]);
    for _ in 0..5 {
        let len = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if len < 1e-12 {
            let mut u = [0f32; 4];
            u[k] = 1.0;
            return u;
        }
        v = v.map(|x| x / len);
        let mut nv = [0f32; 4];
        for a in 0..nch {
            for b in 0..nch {
                nv[a] += cov[a][b] * v[b];
            }
        }
        if nv.iter().map(|x| x * x).sum::<f32>() < 1e-24 {
            break;
        }
        v = nv;
    }
    let len = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
    v.map(|x| x / len)
}

fn least_squares(px: &[[f32; 4]], s: &Spec, idx: &[u8; 16]) -> Option<([f32; 4], [f32; 4])> {
    let ws = interp_weights(s.ib);
    let (mut aa, mut ab, mut bb) = (0f32, 0f32, 0f32);
    let (mut ax, mut bx) = ([0f32; 4], [0f32; 4]);
    for (i, x) in px.iter().enumerate() {
        let t = ws[idx[i] as usize] as f32 / 64.0;
        let a = 1.0 - t;
        aa += a * a;
        ab += a * t;
        bb += t * t;
        for c in 0..s.nch {
            ax[c] += a * x[c];
            bx[c] += t * x[c];
        }
    }
    let det = aa * bb - ab * ab;
    if det.abs() < 1e-6 {
        return None;
    }
    let (mut e0, mut e1) = ([0f32; 4], [0f32; 4]);
    for c in 0..s.nch {
        e0[c] = ((bb * ax[c] - ab * bx[c]) / det).clamp(0.0, 255.0);
        e1[c] = ((aa * bx[c] - ab * ax[c]) / det).clamp(0.0, 255.0);
    }
    Some((e0, e1))
}

fn polish(px: &[[f32; 4]], s: &Spec, mut best: Fit) -> Fit {
    for _ in 0..2 {
        let mut improved = false;
        for e in 0..2 {
            for c in 0..s.nch {
                let max = (1u32 << s.bits[c]) - 1;
                for up in [false, true] {
                    let v = best.q[e][c] as u32;
                    if (up && v == max) || (!up && v == 0) {
                        continue;
                    }
                    let mut q = best.q;
                    q[e][c] = if up { v + 1 } else { v - 1 } as u8;
                    let f = evaluate(px, s, q, best.p);
                    if f.err < best.err {
                        best = f;
                        improved = true;
                    }
                }
            }
        }
        if !improved {
            break;
        }
    }
    best
}

fn fit(px: &[[f32; 4]], s: &Spec) -> Fit {
    if px.iter().all(|x| x[..s.nch] == px[0][..s.nch]) {
        return single(px, s);
    }
    let sw = s.w.map(f32::sqrt);
    let (mean, cov) = moments(px, s.nch, &sw);
    let axis = principal_axis(&cov, s.nch);
    let (mut tmin, mut tmax) = (f32::MAX, f32::MIN);
    for x in px {
        let t: f32 = (0..s.nch).map(|c| (x[c] - mean[c]) * sw[c] * axis[c]).sum();
        tmin = tmin.min(t);
        tmax = tmax.max(t);
    }
    let at = |t: f32| {
        let mut e = [0f32; 4];
        for c in 0..s.nch {
            e[c] = (mean[c] + axis[c] * t / sw[c]).clamp(0.0, 255.0);
        }
        e
    };
    let mut best = try_ends(px, s, at(tmin), at(tmax));
    for _ in 0..1 + s.polish as usize {
        let Some((e0, e1)) = least_squares(px, s, &best.idx) else {
            break;
        };
        let f = try_ends(px, s, e0, e1);
        if f.err < best.err {
            best = f;
        } else {
            break;
        }
    }
    if s.polish && best.err > 0.0 {
        polish(px, s, best)
    } else {
        best
    }
}

fn rank_by_spread(px: &[[f32; 4]; 16], subsets: usize, count: usize) -> Vec<(f32, usize)> {
    let sq = px.map(|x| (0..4).map(|c| x[c] * x[c] * WEIGHTS[c]).sum::<f32>());
    let mut scored: Vec<(f32, usize)> = (0..count)
        .map(|part| {
            let mut n = [0f32; 3];
            let mut sum = [[0f32; 4]; 3];
            let mut tot = 0f32;
            for (i, x) in px.iter().enumerate() {
                let s = subset_of(subsets, part, i);
                n[s] += 1.0;
                for c in 0..4 {
                    sum[s][c] += x[c];
                }
                tot += sq[i];
            }
            for s in 0..subsets {
                tot -= (0..4)
                    .map(|c| sum[s][c] * sum[s][c] * WEIGHTS[c])
                    .sum::<f32>()
                    / n[s];
            }
            (tot, part)
        })
        .collect();
    scored.sort_by(|a, b| a.0.total_cmp(&b.0));
    scored
}

#[derive(Clone, Copy)]
struct Cand {
    mode: u8,
    part: u8,
    rot: u8,
    isel: u8,
    q: [[[u8; 4]; 2]; 3],
    p: [[u8; 2]; 3],
    idx: [u8; 16],
    idx2: [u8; 16],
    dec: [[u8; 4]; 16],
    err: f32,
}

impl Cand {
    fn new(mode: usize, part: usize, rot: usize, isel: usize) -> Cand {
        Cand {
            mode: mode as u8,
            part: part as u8,
            rot: rot as u8,
            isel: isel as u8,
            q: [[[0; 4]; 2]; 3],
            p: [[0; 2]; 3],
            idx: [0; 16],
            idx2: [0; 16],
            dec: [[0; 4]; 16],
            err: 0.0,
        }
    }
}

fn partitioned(px: &[[f32; 4]; 16], mode: usize, part: usize, opaque: bool, polish: bool) -> Cand {
    let m = &MODES[mode];
    let nch = if m.ab > 0 { 4 } else { 3 };
    let s = Spec {
        nch,
        bits: [m.cb, m.cb, m.cb, m.ab],
        pbit: m.pbit,
        ib: m.ib,
        w: WEIGHTS,
        p_one: opaque && m.ab > 0,
        polish,
    };
    let mut c = Cand::new(mode, part, 0, 0);
    for sub in 0..m.subsets {
        let mut ids = [0usize; 16];
        let mut sp = [[0f32; 4]; 16];
        let mut n = 0;
        for (i, x) in px.iter().enumerate() {
            if subset_of(m.subsets, part, i) == sub {
                ids[n] = i;
                sp[n] = *x;
                n += 1;
            }
        }
        let f = fit(&sp[..n], &s);
        c.q[sub] = f.q;
        c.p[sub] = f.p;
        c.err += f.err;
        for (j, &i) in ids[..n].iter().enumerate() {
            c.idx[i] = f.idx[j];
            let mut d = f.dec[j];
            if nch == 3 {
                d[3] = 255;
            }
            c.dec[i] = d;
        }
    }
    c
}

fn separate_alpha(px: &[[f32; 4]; 16], mode: usize, rot: usize, isel: usize, polish: bool) -> Cand {
    let m = &MODES[mode];
    let mut w = WEIGHTS;
    let mut sp = *px;
    if rot > 0 {
        w.swap(rot - 1, 3);
        for x in &mut sp {
            x.swap(rot - 1, 3);
        }
    }
    let ap = sp.map(|x| [x[3], 0.0, 0.0, 0.0]);
    let (cib, aib) = if isel == 1 {
        (m.ib2, m.ib)
    } else {
        (m.ib, m.ib2)
    };
    let cs = Spec {
        nch: 3,
        bits: [m.cb; 4],
        pbit: PBit::None,
        ib: cib,
        w,
        p_one: false,
        polish,
    };
    let als = Spec {
        nch: 1,
        bits: [m.ab; 4],
        pbit: PBit::None,
        ib: aib,
        w: [w[3]; 4],
        p_one: false,
        polish,
    };
    let cf = fit(&sp, &cs);
    let af = fit(&ap, &als);
    let mut c = Cand::new(mode, 0, rot, isel);
    for e in 0..2 {
        c.q[0][e] = [cf.q[e][0], cf.q[e][1], cf.q[e][2], af.q[e][0]];
    }
    c.idx = cf.idx;
    c.idx2 = af.idx;
    for i in 0..16 {
        let mut d = [cf.dec[i][0], cf.dec[i][1], cf.dec[i][2], af.dec[i][0]];
        if rot > 0 {
            d.swap(rot - 1, 3);
        }
        c.dec[i] = d;
    }
    c.err = cf.err + af.err;
    c
}

struct Patches {
    list: [(u16, [u8; 3], u32); 8],
    len: usize,
    swatchy: bool,
}

impl Patches {
    fn of(px: &[[u8; 4]; 16]) -> Patches {
        let rgb = px.map(|p| u32::from_le_bytes([p[0], p[1], p[2], 0]));
        let (mut seen, mut covered) = (0u16, 0);
        let mut out = Patches {
            list: [(0, [0; 3], 0); 8],
            len: 0,
            swatchy: false,
        };
        for i in 0..16 {
            if seen & (1 << i) != 0 {
                continue;
            }
            let mut mask = 1u16 << i;
            for j in i + 1..16 {
                if rgb[j] == rgb[i] {
                    mask |= 1 << j;
                }
            }
            seen |= mask;
            let n = mask.count_ones();
            if n >= 2 {
                covered += n;
                out.list[out.len] = (mask, [px[i][0], px[i][1], px[i][2]], n);
                out.len += 1;
            }
        }
        out.swatchy = out.len >= 3 && covered >= 12;
        out
    }

    fn shift(&self, dec: &[[u8; 4]; 16]) -> f32 {
        let mut worst = 0f32;
        for &(mask, col, n) in &self.list[..self.len] {
            let mut sum = [0u32; 3];
            for (i, d) in dec.iter().enumerate() {
                if mask & (1 << i) != 0 {
                    for c in 0..3 {
                        sum[c] += d[c] as u32;
                    }
                }
            }
            let e: f32 = (0..3)
                .map(|c| {
                    let d = sum[c] as f32 / n as f32 - col[c] as f32;
                    d * d * WEIGHTS[c]
                })
                .sum();
            worst = worst.max(e);
        }
        worst
    }

    fn score(&self, c: &Cand) -> (u8, f32) {
        if !self.swatchy {
            return (0, c.err);
        }
        let s = self.shift(&c.dec);
        let bucket = if s <= SWATCH_GOOD {
            0
        } else if s <= SWATCH_LIMIT {
            1
        } else {
            2
        };
        (bucket, c.err)
    }
}

const fn nearest_w4() -> [u8; 65] {
    let mut t = [0u8; 65];
    let mut v = 0;
    while v <= 64 {
        let mut best = 0;
        let mut k = 1;
        while k < 16 {
            if W4[k].abs_diff(v) < W4[best].abs_diff(v) {
                best = k;
            }
            k += 1;
        }
        t[v as usize] = best as u8;
        v += 1;
    }
    t
}

const NEAREST4: [u8; 65] = nearest_w4();

fn parity_ends(e: [f32; 4], alpha: Option<u8>) -> ([u8; 4], u8) {
    let at = |p: u8| e.map(|v| ((v - p as f32) * 0.5).round().clamp(0.0, 127.0) as u8);
    if let Some(a) = alpha {
        let mut q = at(a & 1);
        q[3] = a >> 1;
        return (q, a & 1);
    }
    let miss = |q: &[u8; 4], p: u8| {
        (0..4)
            .map(|c| (e[c] - (((q[c] as u32) << 1) | p as u32) as f32).powi(2))
            .sum::<f32>()
    };
    let (q0, q1) = (at(0), at(1));
    if miss(&q0, 0) <= miss(&q1, 1) {
        (q0, 0)
    } else {
        (q1, 1)
    }
}

fn mode6_assign(x: &[[f32; 4]; 16], ends: [([u8; 4], u8); 2], g: [f32; 4]) -> Cand {
    let mut c = Cand::new(6, 0, 0, 0);
    let e0: [i32; 4] = std::array::from_fn(|ch| ((ends[0].0[ch] as i32) << 1) | ends[0].1 as i32);
    let e1: [i32; 4] = std::array::from_fn(|ch| ((ends[1].0[ch] as i32) << 1) | ends[1].1 as i32);
    let d: [f32; 4] = std::array::from_fn(|ch| (e1[ch] - e0[ch]) as f32 * g[ch]);
    let dd: f32 = (0..4).map(|ch| d[ch] * (e1[ch] - e0[ch]) as f32).sum();
    let inv = if dd > 0.0 { 64.0 / dd } else { 0.0 };
    for (i, p) in x.iter().enumerate() {
        let t: f32 = (0..4)
            .map(|ch| (p[ch] - e0[ch] as f32) * d[ch])
            .sum::<f32>()
            * inv;
        let k = NEAREST4[t.round().clamp(0.0, 64.0) as usize];
        let w = W4[k as usize] as i32;
        let mut err = 0.0;
        for ch in 0..4 {
            let v = ((64 - w) * e0[ch] + w * e1[ch] + 32) >> 6;
            c.dec[i][ch] = v as u8;
            let diff = p[ch] - v as f32;
            err += diff * diff * WEIGHTS[ch];
        }
        c.idx[i] = k;
        c.err += err;
    }
    c.q[0] = [ends[0].0, ends[1].0];
    c.p[0] = [ends[0].1, ends[1].1];
    c
}

fn mode6_fast(x: &[[f32; 4]; 16], alpha: Option<u8>, aw: f32) -> Cand {
    let sc = [1.0, 1.0, 1.0, aw];
    let g = sc.map(|v| v * v);
    let mut mean = [0f32; 4];
    for p in x {
        for c in 0..4 {
            mean[c] += p[c];
        }
    }
    let mean = mean.map(|v| v / 16.0);
    let mut cov = [[0f32; 4]; 4];
    for p in x {
        let d: [f32; 4] = std::array::from_fn(|c| (p[c] - mean[c]) * sc[c]);
        for a in 0..4 {
            for b in 0..4 {
                cov[a][b] += d[a] * d[b];
            }
        }
    }
    let mut k = 0;
    for c in 1..4 {
        if cov[c][c] > cov[k][k] {
            k = c;
        }
    }
    let mut axis = cov[k];
    for _ in 0..4 {
        let next: [f32; 4] = std::array::from_fn(|a| (0..4).map(|b| cov[a][b] * axis[b]).sum());
        let m = next.iter().fold(0f32, |m, v| m.max(v.abs()));
        if m <= 0.0 {
            break;
        }
        axis = next.map(|v| v / m);
    }
    let len = axis.iter().map(|v| v * v).sum::<f32>().sqrt();
    let axis = if len > 0.0 {
        axis.map(|v| v / len)
    } else {
        [0.5; 4]
    };
    let (mut tmin, mut tmax) = (f32::MAX, f32::MIN);
    for p in x {
        let t: f32 = (0..4).map(|c| (p[c] - mean[c]) * sc[c] * axis[c]).sum();
        tmin = tmin.min(t);
        tmax = tmax.max(t);
    }
    let at = |t: f32| std::array::from_fn(|c| (mean[c] + axis[c] * t / sc[c]).clamp(0.0, 255.0));
    let best = mode6_assign(
        x,
        [parity_ends(at(tmin), alpha), parity_ends(at(tmax), alpha)],
        g,
    );
    if best.err <= 0.0 {
        return best;
    }
    let (mut aa, mut ab, mut bb) = (0f32, 0f32, 0f32);
    let (mut ax, mut bx) = ([0f32; 4], [0f32; 4]);
    for (i, p) in x.iter().enumerate() {
        let t = W4[best.idx[i] as usize] as f32 / 64.0;
        let a = 1.0 - t;
        aa += a * a;
        ab += a * t;
        bb += t * t;
        for c in 0..4 {
            ax[c] += a * p[c];
            bx[c] += t * p[c];
        }
    }
    let det = aa * bb - ab * ab;
    if det.abs() < 1e-6 {
        return best;
    }
    let e0 = std::array::from_fn(|c| ((bb * ax[c] - ab * bx[c]) / det).clamp(0.0, 255.0));
    let e1 = std::array::from_fn(|c| ((aa * bx[c] - ab * ax[c]) / det).clamp(0.0, 255.0));
    let refined = mode6_assign(x, [parity_ends(e0, alpha), parity_ends(e1, alpha)], g);
    if refined.err < best.err {
        refined
    } else {
        best
    }
}

fn flat_table() -> &'static [[u8; 3]; 256] {
    static TABLE: std::sync::OnceLock<[[u8; 3]; 256]> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t = [[0u8, 0, 0]; 256];
        let mut miss = [u32::MAX; 256];
        for q0 in 0..128u32 {
            for q1 in 0..128u32 {
                let v = (43 * expand(q0, 7) + 21 * expand(q1, 7) + 32) >> 6;
                for c in v.saturating_sub(2)..=(v + 2).min(255) {
                    let d = c.abs_diff(v);
                    if d < miss[c as usize] {
                        miss[c as usize] = d;
                        t[c as usize] = [q0 as u8, q1 as u8, v as u8];
                    }
                }
            }
        }
        t
    })
}

fn flat_cand(p: [u8; 4]) -> Cand {
    let t = flat_table();
    let mut c = Cand::new(5, 0, 0, 0);
    let ends = [t[p[0] as usize], t[p[1] as usize], t[p[2] as usize]];
    c.q[0] = [0, 1].map(|e| [ends[0][e], ends[1][e], ends[2][e], p[3]]);
    let dec = [ends[0][2], ends[1][2], ends[2][2], p[3]];
    c.idx = [1; 16];
    c.dec = [dec; 16];
    c.err = 16.0
        * (0..3)
            .map(|ch| (dec[ch] as f32 - p[ch] as f32).powi(2) * WEIGHTS[ch])
            .sum::<f32>();
    c
}

fn rebuild(px: &[[f32; 4]; 16], c: &Cand, opaque: bool, polish: bool) -> Cand {
    match c.mode {
        4 | 5 => separate_alpha(px, c.mode as usize, c.rot as usize, c.isel as usize, polish),
        m => partitioned(px, m as usize, c.part as usize, opaque, polish),
    }
}

fn deep_search(px: &[[f32; 4]; 16], opaque: bool, patches: &Patches, start: Cand) -> Cand {
    let mut best = (patches.score(&start), start);
    let keep = |best: &mut ((u8, f32), Cand), c: Cand| {
        let s = patches.score(&c);
        if s < best.0 {
            *best = (s, c);
        }
    };
    if opaque {
        for &(_, part) in rank_by_spread(px, 3, 64).iter().take(3) {
            keep(&mut best, partitioned(px, 2, part, true, false));
        }
        for &(_, part) in rank_by_spread(px, 2, 64).iter().take(2) {
            keep(&mut best, partitioned(px, 3, part, true, false));
            keep(&mut best, partitioned(px, 1, part, true, false));
        }
    } else {
        for &(_, part) in rank_by_spread(px, 2, 64).iter().take(6) {
            keep(&mut best, partitioned(px, 7, part, false, false));
        }
    }
    if best.1.mode != 6 {
        let winner = best.1;
        keep(&mut best, rebuild(px, &winner, opaque, true));
    }
    best.1
}

fn best_cand(px8: &[[u8; 4]; 16]) -> Cand {
    if px8.iter().all(|p| *p == px8[0]) {
        return flat_cand(px8[0]);
    }
    let px = px8.map(|p| p.map(|v| v as f32));
    let one_alpha = px8.iter().all(|p| p[3] == px8[0][3]);
    let opaque = one_alpha && px8[0][3] == 255;
    let mut best = mode6_fast(&px, one_alpha.then_some(px8[0][3]), 1.0);
    let alpha_miss = |c: &Cand| {
        (0..16)
            .map(|i| (px8[i][3] as i32 - c.dec[i][3] as i32).pow(2))
            .sum::<i32>()
    };
    if !one_alpha && alpha_miss(&best) > ALPHA_RETRY {
        for c in [
            mode6_fast(&px, None, 4.0),
            separate_alpha(&px, 5, 0, 0, false),
            separate_alpha(&px, 4, 0, 0, false),
        ] {
            if c.err < best.err {
                best = c;
            }
        }
    }
    let patches = Patches::of(px8);
    if patches.swatchy && patches.shift(&best.dec) > SWATCH_GOOD {
        best = deep_search(&px, opaque, &patches, best);
    }
    best
}

#[derive(Default)]
struct Bits {
    v: u128,
    n: u32,
}

impl Bits {
    fn put(&mut self, val: u32, bits: u32) {
        self.v |= ((val as u128) & ((1u128 << bits) - 1)) << self.n;
        self.n += bits;
    }
}

fn pack(cand: &Cand) -> [u8; 16] {
    let mut c = *cand;
    let mode = c.mode as usize;
    let m = &MODES[mode];
    let part = c.part as usize;
    let anc = anchors(m.subsets, part);
    let separate = m.ib2 > 0;
    let (cib, aib) = if c.isel == 1 {
        (m.ib2, m.ib)
    } else {
        (m.ib, m.ib2)
    };
    let colour_chans = if separate { 3 } else { 4 };
    for (s, &a) in anc.iter().enumerate().take(m.subsets) {
        let top = (1u8 << cib) - 1;
        if c.idx[a] > top >> 1 {
            for ch in 0..colour_chans {
                let t = c.q[s][0][ch];
                c.q[s][0][ch] = c.q[s][1][ch];
                c.q[s][1][ch] = t;
            }
            c.p[s].swap(0, 1);
            for i in 0..16 {
                if subset_of(m.subsets, part, i) == s {
                    c.idx[i] = top - c.idx[i];
                }
            }
        }
    }
    if separate {
        let top = (1u8 << aib) - 1;
        if c.idx2[0] > top >> 1 {
            let t = c.q[0][0][3];
            c.q[0][0][3] = c.q[0][1][3];
            c.q[0][1][3] = t;
            for v in &mut c.idx2 {
                *v = top - *v;
            }
        }
    }
    let mut b = Bits::default();
    b.put(1 << mode, mode as u32 + 1);
    b.put(part as u32, m.part_bits);
    if separate {
        b.put(c.rot as u32, 2);
        if mode == 4 {
            b.put(c.isel as u32, 1);
        }
    }
    for ch in 0..3 {
        for s in 0..m.subsets {
            for e in 0..2 {
                b.put(c.q[s][e][ch] as u32, m.cb);
            }
        }
    }
    if m.ab > 0 {
        for s in 0..m.subsets {
            for e in 0..2 {
                b.put(c.q[s][e][3] as u32, m.ab);
            }
        }
    }
    match m.pbit {
        PBit::Unique => {
            for s in 0..m.subsets {
                b.put(c.p[s][0] as u32, 1);
                b.put(c.p[s][1] as u32, 1);
            }
        }
        PBit::Shared => {
            for s in 0..m.subsets {
                b.put(c.p[s][0] as u32, 1);
            }
        }
        PBit::None => {}
    }
    let (first, second) = if c.isel == 1 {
        (&c.idx2, &c.idx)
    } else {
        (&c.idx, &c.idx2)
    };
    for (i, &v) in first.iter().enumerate() {
        let anchor = anc[..m.subsets].contains(&i);
        b.put(v as u32, m.ib - anchor as u32);
    }
    if separate {
        for (i, &v) in second.iter().enumerate() {
            b.put(v as u32, m.ib2 - (i == 0) as u32);
        }
    }
    debug_assert_eq!(b.n, 128);
    b.v.to_le_bytes()
}

fn gather(rgba: &[u8], w: usize, h: usize, bx: usize, by: usize) -> [[u8; 4]; 16] {
    let mut px = [[0u8; 4]; 16];
    for y in 0..4 {
        let sy = (by * 4 + y).min(h - 1);
        for x in 0..4 {
            let sx = (bx * 4 + x).min(w - 1);
            let o = (sy * w + sx) * 4;
            px[y * 4 + x].copy_from_slice(&rgba[o..o + 4]);
        }
    }
    px
}

pub fn encode(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let (w, h) = (width.max(1) as usize, height.max(1) as usize);
    let (bw, bh) = (w.div_ceil(4), h.div_ceil(4));
    let mut out = vec![0u8; bw * bh * 16];
    let row = |by: usize, dst: &mut [u8]| {
        for bx in 0..bw {
            dst[bx * 16..(bx + 1) * 16]
                .copy_from_slice(&pack(&best_cand(&gather(rgba, w, h, bx, by))));
        }
    };
    if bw * bh >= 4096 {
        out.par_chunks_mut(bw * 16)
            .enumerate()
            .for_each(|(by, dst)| row(by, dst));
    } else {
        for (by, dst) in out.chunks_mut(bw * 16).enumerate() {
            row(by, dst);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bc::{self, Bc};

    fn decode(blocks: &[u8], w: u32, h: u32) -> Vec<u8> {
        let mut px = vec![0u32; (w * h) as usize];
        texture2ddecoder::decode_bc7(blocks, w as usize, h as usize, &mut px).unwrap();
        px.iter()
            .flat_map(|p| {
                let [b, g, r, a] = p.to_le_bytes();
                [r, g, b, a]
            })
            .collect()
    }

    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 32) as u32
        }

        fn byte(&mut self) -> u8 {
            self.next() as u8
        }
    }

    fn psnr(a: &[u8], b: &[u8], chans: usize) -> f64 {
        let (mut sum, mut n) = (0f64, 0f64);
        for (x, y) in a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0.iter()) {
            for c in 0..chans {
                let d = x[c] as f64 - y[c] as f64;
                sum += d * d;
                n += 1.0;
            }
        }
        if sum == 0.0 {
            99.0
        } else {
            10.0 * (255.0f64 * 255.0 / (sum / n)).log10()
        }
    }

    fn bc_round_trip(rgba: &[u8], w: u32, h: u32, format: Bc) -> Vec<u8> {
        let (blocks, _, _) = bc::encode(rgba, w, h, format);
        bc::decode(&blocks, w, h, format)
    }

    fn picture(blocks: &[[[u8; 4]; 16]], per_row: usize) -> (Vec<u8>, u32, u32) {
        let (w, h) = (per_row * 4, blocks.len().div_ceil(per_row) * 4);
        let mut rgba = vec![0u8; w * h * 4];
        for (b, px) in blocks.iter().enumerate() {
            let (bx, by) = (b % per_row, b / per_row);
            for (i, p) in px.iter().enumerate() {
                let o = ((by * 4 + i / 4) * w + bx * 4 + i % 4) * 4;
                rgba[o..o + 4].copy_from_slice(p);
            }
        }
        (rgba, w as u32, h as u32)
    }

    #[test]
    fn flat_blocks_come_back_exactly() {
        let mut rng = Rng(0x1234_5678_9abc_def0);
        let mut colours = vec![
            [0, 0, 0, 255],
            [255, 255, 255, 255],
            [0, 0, 0, 0],
            [1, 2, 3, 0],
            [128, 127, 129, 200],
            [254, 1, 253, 1],
        ];
        for v in 0..=255u8 {
            colours.push([v, 255 - v, v / 2, 255]);
            colours.push([v, v, v, v]);
        }
        for _ in 0..500 {
            let a = if rng.next().is_multiple_of(2) {
                255
            } else {
                rng.byte()
            };
            colours.push([rng.byte(), rng.byte(), rng.byte(), a]);
        }
        let (mut exact, mut total) = (0, 0);
        for col in colours {
            let rgba: Vec<u8> = (0..16).flat_map(|_| col).collect();
            let dec = decode(&encode(&rgba, 4, 4), 4, 4);
            for p in dec.as_chunks::<4>().0.iter() {
                for c in 0..4 {
                    assert!(
                        (p[c] as i32 - col[c] as i32).abs() <= 1,
                        "{col:?} came back as {p:?}"
                    );
                    total += 1;
                    exact += (p[c] == col[c]) as u32;
                }
                if col[3] == 255 {
                    assert_eq!(p[3], 255);
                }
            }
        }
        assert!(
            exact * 100 >= total * 99,
            "{exact} of {total} channels exact"
        );
    }

    #[test]
    fn every_mode_packs_as_the_reference_decoder_reads_it() {
        let mut rng = Rng(0x0dd_ba11_cafe_f00d);
        let mut seen = [0u32; 8];
        for round in 0..400 {
            let palette: Vec<[u8; 4]> = (0..1 + round % 5)
                .map(|_| [rng.byte(), rng.byte(), rng.byte(), rng.byte()])
                .collect();
            let mut px8 = [[0u8; 4]; 16];
            for p in &mut px8 {
                *p = if round % 3 == 0 {
                    [rng.byte(), rng.byte(), rng.byte(), rng.byte()]
                } else {
                    palette[rng.next() as usize % palette.len()]
                };
            }
            let opaque = round % 2 == 0;
            if opaque {
                for p in &mut px8 {
                    p[3] = 255;
                }
            }
            let px = px8.map(|p| p.map(|v| v as f32));
            let part = rng.next() as usize % 64;
            let rot = rng.next() as usize % 4;
            let cands = [
                partitioned(&px, 0, part % 16, true, round % 4 < 2),
                partitioned(&px, 1, part, true, round % 4 < 2),
                partitioned(&px, 2, part, true, round % 4 < 2),
                partitioned(&px, 3, part, true, round % 4 < 2),
                separate_alpha(&px, 4, rot, round % 2, round % 4 < 2),
                separate_alpha(&px, 5, rot, 0, round % 4 < 2),
                partitioned(&px, 6, 0, opaque, round % 4 < 2),
                partitioned(&px, 7, part, opaque, round % 4 < 2),
                best_cand(&px8),
            ];
            for c in cands {
                let block = pack(&c);
                assert_eq!(block[0].trailing_zeros(), c.mode as u32);
                let dec = decode(&block, 4, 4);
                for i in 0..16 {
                    assert_eq!(
                        &dec[i * 4..i * 4 + 4],
                        &c.dec[i],
                        "mode {} part {} rot {} isel {}",
                        c.mode,
                        c.part,
                        c.rot,
                        c.isel
                    );
                }
                seen[c.mode as usize] += 1;
            }
        }
        assert!(seen.iter().all(|&n| n > 0));
    }

    #[test]
    fn the_encoder_picks_every_kind_of_mode() {
        let mut rng = Rng(0x5eed_5eed_5eed_5eed);
        let mut seen = [false; 8];
        for round in 0..3000 {
            let mut px8 = [[0u8; 4]; 16];
            let base = [rng.byte(), rng.byte(), rng.byte(), rng.byte()];
            let palette: Vec<[u8; 4]> = (0..2 + round % 3)
                .map(|_| [rng.byte(), rng.byte(), rng.byte(), rng.byte()])
                .collect();
            for (i, p) in px8.iter_mut().enumerate() {
                *p = match round % 4 {
                    0 => palette[rng.next() as usize % palette.len()],
                    1 => base.map(|v| v.saturating_add((rng.next() % 9) as u8)),
                    2 => [base[0], base[1], base[2], (i * 17) as u8],
                    _ => [rng.byte(), rng.byte(), rng.byte(), base[3]],
                };
                if round % 8 < 4 {
                    p[3] = 255;
                }
            }
            let c = best_cand(&px8);
            seen[c.mode as usize] = true;
        }
        assert!(seen.iter().filter(|&&s| s).count() >= 6, "{seen:?}");
    }

    #[test]
    fn noise_measures_at_least_as_well_as_bc1() {
        let mut rng = Rng(42);
        let (w, h) = (64u32, 64u32);
        let rgba: Vec<u8> = (0..w * h)
            .flat_map(|_| [rng.byte(), rng.byte(), rng.byte(), 255])
            .collect();
        let ours = decode(&encode(&rgba, w, h), w, h);
        let theirs = bc_round_trip(&rgba, w, h, Bc::Bc1 { punch: false });
        assert!(
            psnr(&rgba, &ours, 3) >= psnr(&rgba, &theirs, 3),
            "{} < {}",
            psnr(&rgba, &ours, 3),
            psnr(&rgba, &theirs, 3)
        );
        assert!(ours.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
    }

    #[test]
    fn noise_with_alpha_measures_at_least_as_well_as_bc3() {
        let mut rng = Rng(7);
        let (w, h) = (64u32, 64u32);
        let rgba: Vec<u8> = (0..w * h)
            .flat_map(|_| [rng.byte(), rng.byte(), rng.byte(), rng.byte()])
            .collect();
        let ours = decode(&encode(&rgba, w, h), w, h);
        let theirs = bc_round_trip(&rgba, w, h, Bc::Bc3);
        assert!(
            psnr(&rgba, &ours, 4) >= psnr(&rgba, &theirs, 4),
            "{} < {}",
            psnr(&rgba, &ours, 4),
            psnr(&rgba, &theirs, 4)
        );
        assert!(psnr(&rgba, &ours, 3) >= psnr(&rgba, &theirs, 3));
    }

    #[test]
    fn a_soft_picture_with_alpha_measures_at_least_as_well_as_bc3() {
        let (w, h) = (64u32, 64u32);
        let rgba: Vec<u8> = (0..w * h)
            .flat_map(|i| {
                let (x, y) = ((i % w) as f32, (i / w) as f32);
                [
                    (128.0 + 100.0 * (x * 0.3).sin()) as u8,
                    (y * 4.0) as u8,
                    (x * 2.0 + y) as u8,
                    (255.0 - x * 3.0).max(0.0) as u8,
                ]
            })
            .collect();
        let ours = decode(&encode(&rgba, w, h), w, h);
        let theirs = bc_round_trip(&rgba, w, h, Bc::Bc3);
        assert!(psnr(&rgba, &ours, 4) >= psnr(&rgba, &theirs, 4));
    }

    #[test]
    fn a_gradient_stays_smooth() {
        let (w, h) = (64u32, 64u32);
        let rgba: Vec<u8> = (0..w * h)
            .flat_map(|i| {
                [
                    (i % w * 4) as u8,
                    (i / w * 4) as u8,
                    ((i % w + i / w) * 2) as u8,
                    255,
                ]
            })
            .collect();
        let ours = decode(&encode(&rgba, w, h), w, h);
        let theirs = bc_round_trip(&rgba, w, h, Bc::Bc1 { punch: false });
        let (p, q) = (psnr(&rgba, &ours, 3), psnr(&rgba, &theirs, 3));
        assert!(p >= 38.0 && p >= q, "{p} {q}");
    }

    const RED: [u8; 4] = [200, 30, 30, 255];
    const GREEN: [u8; 4] = [40, 160, 60, 255];
    const WHITE: [u8; 4] = [240, 240, 240, 255];

    fn swatch_blocks() -> Vec<[[u8; 4]; 16]> {
        let (r, g, w) = (RED, GREEN, WHITE);
        let mut rng = Rng(99);
        let mut noisy = [
            r, r, g, g, r, r, g, g, w, w, [0; 4], [0; 4], w, w, [0; 4], [0; 4],
        ];
        for p in &mut noisy {
            if p[3] == 0 {
                *p = [rng.byte(), rng.byte(), rng.byte(), 255];
            }
        }
        vec![
            [r, r, g, g, r, r, g, g, w, w, r, r, w, w, r, r],
            [r, g, w, r, g, w, r, g, w, r, g, w, r, g, w, r],
            [w, w, w, w, r, r, r, r, g, g, g, g, w, w, w, w],
            noisy,
        ]
    }

    #[test]
    fn three_swatches_keep_their_colours() {
        let (rgba, w, h) = picture(&swatch_blocks(), 2);
        let theirs = bc_round_trip(&rgba, w, h, Bc::Bc1 { punch: false });
        assert!(bc::worst_patch_shift(&rgba, &theirs, w, h) > 1600.0);
        let ours = decode(&encode(&rgba, w, h), w, h);
        let shift = bc::worst_patch_shift(&rgba, &ours, w, h);
        assert!(shift <= 64.0, "{shift}");
    }

    #[test]
    fn swatches_with_alpha_keep_their_colours() {
        let (r, g, w) = ([200, 30, 30, 255], [40, 160, 60, 128], [240, 240, 240, 0]);
        let blocks = [
            [r, r, g, g, r, r, g, g, w, w, r, r, w, w, r, r],
            [w, w, w, w, r, r, r, r, g, g, g, g, w, w, w, w],
        ];
        let (rgba, wd, ht) = picture(&blocks, 2);
        let ours = decode(&encode(&rgba, wd, ht), wd, ht);
        let shift = bc::worst_patch_shift(&rgba, &ours, wd, ht);
        assert!(shift <= 64.0, "{shift}");
        for (s, d) in rgba
            .as_chunks::<4>()
            .0
            .iter()
            .zip(ours.as_chunks::<4>().0.iter())
        {
            assert!((s[3] as i32 - d[3] as i32).abs() <= 4, "{s:?} {d:?}");
        }
    }

    #[test]
    fn partial_pictures_repeat_their_edges() {
        for (w, h) in [(6u32, 5u32), (1, 1), (3, 7), (9, 2)] {
            let rgba: Vec<u8> = (0..w * h)
                .flat_map(|i| [(i * 23 % 256) as u8, (i * 7 % 256) as u8, 90, 255])
                .collect();
            let blocks = encode(&rgba, w, h);
            assert_eq!(blocks.len(), (w.div_ceil(4) * h.div_ceil(4) * 16) as usize);
            let dec = decode(&blocks, w, h);
            let theirs = bc_round_trip(&rgba, w, h, Bc::Bc1 { punch: false });
            let (p, q) = (psnr(&rgba, &dec, 3), psnr(&rgba, &theirs, 3));
            assert!(p >= q, "{w}x{h}: {p} {q}");
            assert!(dec.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
        }
    }

    #[test]
    fn big_pictures_encode_each_block_as_alone() {
        let (w, h) = (256u32, 256u32);
        let mut rng = Rng(3);
        let rgba: Vec<u8> = (0..w * h)
            .flat_map(|i| [(i % w) as u8, (i / w) as u8, rng.byte() / 8, 255])
            .collect();
        let blocks = encode(&rgba, w, h);
        for (bx, by) in [(0usize, 0usize), (17, 3), (63, 63)] {
            let px = gather(&rgba, w as usize, h as usize, bx, by);
            let one: Vec<u8> = px.iter().flatten().copied().collect();
            assert_eq!(
                &blocks[(by * 64 + bx) * 16..][..16],
                &encode(&one, 4, 4)[..]
            );
        }
    }
}
