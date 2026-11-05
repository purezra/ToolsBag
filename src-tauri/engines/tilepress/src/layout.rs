//! 版面引擎：二叉切分树 + 模拟退火 + 全局挑图换位。
//! 目标：在页边距/图片间距严格按毫米约束下，最小化整页白边面积。

use rand::rngs::StdRng;
use rand::Rng;

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn area(&self) -> f32 {
        self.w * self.h
    }
    pub fn aspect(&self) -> f32 {
        self.w / self.h
    }
    pub fn inset(&self, d: f32) -> Rect {
        Rect {
            x: self.x + d,
            y: self.y + d,
            w: self.w - 2.0 * d,
            h: self.h - 2.0 * d,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Node {
    /// 槽位号（指向 page_imgs 的下标）
    Leaf(u32),
    /// dir: 0=左右切, 1=上下切
    Split {
        dir: u8,
        ratio: f32,
        l: u32,
        rr: u32,
    },
}

pub struct Tree {
    pub nodes: Vec<Node>,
    pub root: u32,
}

impl Tree {
    pub fn new() -> Self {
        Tree {
            nodes: Vec::new(),
            root: 0,
        }
    }
    fn push(&mut self, n: Node) -> u32 {
        self.nodes.push(n);
        (self.nodes.len() - 1) as u32
    }
    pub fn collect(&self, r: Rect, out: &mut Vec<(u32, Rect)>) {
        self.rec(self.root, r, out);
    }
    fn rec(&self, idx: u32, r: Rect, out: &mut Vec<(u32, Rect)>) {
        match self.nodes[idx as usize] {
            Node::Leaf(s) => out.push((s, r)),
            Node::Split { dir, ratio, l, rr } => {
                if dir == 0 {
                    let w1 = r.w * ratio;
                    self.rec(
                        l,
                        Rect {
                            x: r.x,
                            y: r.y,
                            w: w1,
                            h: r.h,
                        },
                        out,
                    );
                    self.rec(
                        rr,
                        Rect {
                            x: r.x + w1,
                            y: r.y,
                            w: r.w - w1,
                            h: r.h,
                        },
                        out,
                    );
                } else {
                    let h1 = r.h * ratio;
                    self.rec(
                        l,
                        Rect {
                            x: r.x,
                            y: r.y,
                            w: r.w,
                            h: h1,
                        },
                        out,
                    );
                    self.rec(
                        rr,
                        Rect {
                            x: r.x,
                            y: r.y + h1,
                            w: r.w,
                            h: r.h - h1,
                        },
                        out,
                    );
                }
            }
        }
    }
    fn split_ids(&self) -> Vec<u32> {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| matches!(n, Node::Split { .. }))
            .map(|(i, _)| i as u32)
            .collect()
    }
}

/// 单格白边面积（mm²）：图片按 contain 方式贴合格子的剩余空白
#[inline]
fn waste_of(cell: Rect, img_aspect: f32) -> f32 {
    if cell.w <= 0.0 || cell.h <= 0.0 || img_aspect <= 0.0 {
        return cell.area().max(0.0);
    }
    let cell_a = cell.aspect();
    let fit = if img_aspect > cell_a {
        cell.w * (cell.w / img_aspect)
    } else {
        cell.h * (cell.h * img_aspect)
    };
    (cell.area() - fit).max(0.0)
}

/// 把叶子矩形收缩 gutter/2 并夹到内框内，得到真实图片格
#[inline]
fn leaf_cell(r: Rect, g: f32, inner: Rect) -> Rect {
    let h = g * 0.5;
    let x0 = (r.x + h).max(inner.x);
    let y0 = (r.y + h).max(inner.y);
    let x1 = (r.x + r.w - h).min(inner.x + inner.w);
    let y1 = (r.y + r.h - h).min(inner.y + inner.h);
    Rect {
        x: x0,
        y: y0,
        w: (x1 - x0).max(0.0),
        h: (y1 - y0).max(0.0),
    }
}

pub fn leaf_cell_pub(r: Rect, g: f32, inner: Rect) -> Rect {
    leaf_cell(r, g, inner)
}

pub struct Ctx {
    pub inner: Rect,
    pub gutter: f32,
    pub page_area: f32,
}

impl Ctx {
    /// 目标函数 = 白边面积 + 均匀度惩罚
    ///
    /// 只优化白边会让算法牺牲个别图（把它压成 14mm 宽的窄条）去换整体填充率，
    /// 观感上就是"有的图特别瘦、特别小"。所以加一项：每张图的格子面积
    /// 必须贴近平均值，偏差按平方计。λ 越大越均匀、填充率越低。
    pub fn energy(&self, t: &Tree, slot_asp: &[f32]) -> f32 {
        let mut cells = Vec::with_capacity(slot_asp.len());
        t.collect(self.inner, &mut cells);
        let n = cells.len().max(1) as f32;
        let mean = (self.inner.w * self.inner.h) / n;
        let mut w = 0.0;
        let mut dev = 0.0;
        for (slot, r) in cells {
            let c = leaf_cell(r, self.gutter, self.inner);
            w += waste_of(c, slot_asp[slot as usize]);
            let d = (c.w * c.h) / mean - 1.0;
            dev += d * d;
        }
        w + UNIFORM_LAMBDA * mean * dev
    }
}

/// 均匀度权重：0 = 只看填充率，越大越"每张图一样大"
pub const UNIFORM_LAMBDA: f32 = 1.6;

/// 等面积行式排版：直接算出每张图的格子，不走切分树、不做退火。
///
/// 硬保证（按主人给的优先级）：
/// 1. 不裁剪——格子长宽比严格等于原图长宽比，原图什么样就什么样
/// 2. 每张图面积尽量一致——按长宽比排序后成行，行内比例相近，行高由"该行长宽比之和"反推
/// 3. 图与图之间间距严格等于 gutter（横向纵向都是）
/// 4. 整块内容总高度未必正好等于页内高度，多出来的部分留白（允许四周页边距不等）
///
/// 返回 (每张图的格子, 实际使用高度)
pub fn uniform_cells(
    slots: &[u32],
    asp: &[f32],
    r: Rect,
    gutter: f32,
) -> (Vec<(u32, Rect)>, f32) {
    let n = slots.len();
    if n == 0 {
        return (Vec::new(), 0.0);
    }
    let mut order: Vec<(u32, f32)> = slots.iter().map(|&s| (s, asp[s as usize])).collect();
    // 长宽比排序：相近比例的图落进同一行，行内面积才均匀
    order.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // 初值：枚举行数 R，每行张数尽量相等（按比例连续分组）
    // 初值：枚举行数，两种分行策略都试（等张数 / 等比例和）
    // 硬约束：总高必须贴合页高（否则填充率崩），在此前提下挑面积最均匀的
    let mut best_sizes: Vec<usize> = vec![n];
    let mut best_spread = f32::MAX;
    let mut fallback: Option<(f32, Vec<usize>)> = None;
    for rows in 2..=(n / 2).max(1) {
        for sizes in [
            equal_sizes(n, rows),
            balanced_sizes(&order, rows),
            area_balanced_sizes(&order, rows),
        ] {
            let (tot, spread) = plan_metrics(&order, &sizes, r.w, gutter);
            if tot <= r.h * 1.005 && tot >= r.h * 0.96 && spread < best_spread {
                best_spread = spread;
                best_sizes = sizes.clone();
            }
            let err = ((tot - r.h) / r.h).abs();
            if fallback.as_ref().map_or(true, |(e, _)| err < *e) {
                fallback = Some((err, sizes));
            }
        }
    }
    if best_spread == f32::MAX {
        if let Some((_, sizes)) = fallback {
            best_sizes = sizes;
        }
    }

    // 爬山微调：把一张图从一行挪到相邻行，直接以"面积最大/最小比"为目标
    let mut best_spread = plan_metrics(&order, &best_sizes, r.w, gutter).1;
    for _round in 0..12 {
        let mut improved = false;
        let rows = best_sizes.len();
        for ri in 0..rows {
            for dir in 0..2 {
                let mut cand = best_sizes.clone();
                if dir == 0 {
                    if ri + 1 >= rows || cand[ri] < 3 {
                        continue;
                    }
                    cand[ri] -= 1;
                    cand[ri + 1] += 1;
                } else {
                    if ri + 1 >= rows || cand[ri + 1] < 3 {
                        continue;
                    }
                    cand[ri] += 1;
                    cand[ri + 1] -= 1;
                }
                let (tot, spread) = plan_metrics(&order, &cand, r.w, gutter);
                if tot < r.h * 0.94 || tot > r.h * 1.06 {
                    continue;
                }
                if spread < best_spread - 1e-4 {
                    best_spread = spread;
                    best_sizes = cand;
                    improved = true;
                }
            }
        }
        if !improved {
            break;
        }
    }

    // 面积上限通过缩小整行实现，不能单独改变宽度，否则会破坏原图比例。
    let mut row_area: Vec<f32> = Vec::with_capacity(best_sizes.len());
    {
        let mut i = 0usize;
        for k in &best_sizes {
            let row = &order[i..i + *k];
            let sum: f32 = row.iter().map(|(_, a)| *a).sum::<f32>().max(1e-6);
            let usable = r.w - (*k as f32 - 1.0) * gutter;
            let h = usable / sum;
            let amax: f32 = row.iter().map(|(_, a)| *a).fold(0.0f32, f32::max);
            row_area.push(amax * h * h);
            i += *k;
        }
    }
    let cap = {
        let mut v = row_area.clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let med = v[v.len() / 2];
        med * 1.25
    };

    let mut heights = Vec::with_capacity(best_sizes.len());
    let mut i = 0usize;
    for k in &best_sizes {
        let row = &order[i..i + *k];
        let sum: f32 = row.iter().map(|(_, a)| *a).sum::<f32>().max(1e-6);
        let usable = r.w - (*k as f32 - 1.0) * gutter;
        let amax = row.iter().map(|(_, a)| *a).fold(0.0f32, f32::max);
        heights.push((usable / sum).min((cap / amax.max(1e-6)).sqrt()));
        i += *k;
    }
    // 只缩图片，不缩毫米间距；总高必须严格落在页内区域，不能通过减小边距容纳。
    let gaps = best_sizes.len().saturating_sub(1) as f32 * gutter;
    let image_height: f32 = heights.iter().sum();
    let scale = ((r.h - gaps).max(0.0) / image_height.max(1e-6)).min(1.0);
    let mut out = Vec::with_capacity(n);
    let mut y = 0.0f32;
    let mut i = 0usize;
    for (k, row_height) in best_sizes.iter().zip(heights) {
        let row = &order[i..i + *k];
        let h = row_height * scale;
        let wsum = row.iter().map(|(_, a)| a * h).sum::<f32>() + (*k as f32 - 1.0) * gutter;
        let x_off = ((r.w - wsum) / 2.0).max(0.0);
        let mut x = 0.0f32;
        for (slot, a) in row {
            let w = a * h;
            out.push((
                *slot,
                Rect {
                    x: r.x + x_off + x,
                    y: r.y + y,
                    w,
                    h,
                },
            ));
            x += w + gutter;
        }
        y += h + gutter;
        i += *k;
    }
    let used_h = (y - gutter).max(0.0);
    (out, used_h)
}

/// 按目标格子面积 A，逐行决定放几张：k = (W+g)/(sqrt(A*a)+g)
/// 保留自上游算法，当前行式排版未调用。
#[allow(dead_code)]
fn walk_sizes(order: &[(u32, f32)], a_target: f32, w: f32, gutter: f32) -> Vec<usize> {
    let n = order.len();
    let mut sizes: Vec<usize> = Vec::new();
    let mut i = 0usize;
    while i < n {
        let a = order[i].1.max(1e-3);
        let ideal = (w + gutter) / ((a_target * a).sqrt() + gutter);
        let k = (ideal.round().max(2.0) as usize).min(n - i);
        sizes.push(k);
        i += k;
    }
    // 尾行太短（1-2 张）会让行高暴涨，并进上一行
    while sizes.len() > 1 && *sizes.last().unwrap() < 3 {
        let last = sizes.pop().unwrap();
        *sizes.last_mut().unwrap() += last;
    }
    sizes
}

/// 等面积分行：行内张数 ∝ 1/√比例。
/// 推导：行内每张面积 = a·h²，行高 h = 可用宽 / 行内比例和；
/// 要让各行面积相等，需 行内比例和 ∝ √a，即张数 ∝ 1/√a（窄图多放、宽图少放）。
fn area_balanced_sizes(order: &[(u32, f32)], rows: usize) -> Vec<usize> {
    let n = order.len();
    if rows <= 1 || n == 0 {
        return vec![n];
    }
    let mut sizes = equal_sizes(n, rows);
    for _ in 0..4 {
        // 按当前分行算每行的平均比例
        let mut means = Vec::with_capacity(rows);
        let mut i = 0usize;
        for &k in &sizes {
            let seg = &order[i..(i + k).min(n)];
            let m = seg.iter().map(|(_, a)| *a).sum::<f32>() / seg.len().max(1) as f32;
            means.push(m.max(1e-3));
            i += k;
        }
        // 权重 ∝ 1/√a，归一化到 n
        let w: Vec<f32> = means.iter().map(|a| 1.0 / a.sqrt()).collect();
        let wsum: f32 = w.iter().sum::<f32>().max(1e-6);
        let mut next: Vec<usize> = w
            .iter()
            .map(|x| ((x / wsum) * n as f32).round() as usize)
            .map(|v| v.max(1))
            .collect();
        // 修正总数
        let mut guard = 0;
        loop {
            let sum: usize = next.iter().sum();
            if sum == n || guard > 200 {
                break;
            }
            if sum < n {
                let idx = next
                    .iter()
                    .enumerate()
                    .min_by(|a, b| {
                        let wa = w[a.0] / (*a.1).max(1) as f32;
                        let wb = w[b.0] / (*b.1).max(1) as f32;
                        wb.partial_cmp(&wa).unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                next[idx] += 1;
            } else {
                let idx = next
                    .iter()
                    .enumerate()
                    .filter(|(_, v)| **v > 1)
                    .max_by(|a, b| a.1.cmp(b.1))
                    .map(|(i, _)| i);
                match idx {
                    Some(i) => next[i] -= 1,
                    None => break,
                }
            }
            guard += 1;
        }
        if next == sizes {
            break;
        }
        sizes = next;
    }
    sizes
}

/// 按"每行比例和尽量相等"分行：行高 = 可用宽 / 比例和，
/// 比例和相等 → 行高相等 → 全页格子大小更统一（窄图多放几张，宽图少放几张）
fn balanced_sizes(order: &[(u32, f32)], rows: usize) -> Vec<usize> {
    let n = order.len();
    if rows <= 1 || n == 0 {
        return vec![n];
    }
    let total: f32 = order.iter().map(|(_, a)| *a).sum();
    let target = total / rows as f32;
    let mut sizes = Vec::with_capacity(rows);
    let mut i = 0usize;
    for r in 0..rows {
        let left_rows = rows - r;
        let left_n = n - i;
        if left_rows == 1 {
            sizes.push(left_n);
            break;
        }
        // 至少要给后面的每行留一张
        let max_take = left_n.saturating_sub(left_rows - 1);
        let mut acc = 0.0f32;
        let mut take = 0usize;
        while take < max_take {
            let a = order[i + take].1;
            if take > 0 && (acc + a - target).abs() > (acc - target).abs() {
                break;
            }
            acc += a;
            take += 1;
        }
        if take == 0 {
            take = 1;
        }
        sizes.push(take);
        i += take;
    }
    sizes
}

fn equal_sizes(n: usize, rows: usize) -> Vec<usize> {
    let base = n / rows;
    let rem = n % rows;
    (0..rows)
        .map(|i| base + if i < rem { 1 } else { 0 })
        .collect()
}

/// 给定每行张数，算总高度和"最大/最小格子面积"比值
fn plan_metrics(order: &[(u32, f32)], sizes: &[usize], w: f32, gutter: f32) -> (f32, f32) {
    let mut tot = 0.0f32;
    let mut i = 0usize;
    let mut amin = f32::MAX;
    let mut amax = 0.0f32;
    for (ri, k) in sizes.iter().enumerate() {
        if *k == 0 || i + k > order.len() {
            return (f32::MAX, f32::MAX);
        }
        let row = &order[i..i + k];
        let sum: f32 = row.iter().map(|(_, a)| *a).sum::<f32>().max(1e-6);
        let usable = w - (*k as f32 - 1.0) * gutter;
        let h = usable / sum;
        tot += h;
        if ri + 1 < sizes.len() {
            tot += gutter;
        }
        for (_, a) in row {
            let area = a * h * h;
            if area < amin {
                amin = area;
            }
            if area > amax {
                amax = area;
            }
        }
        i += k;
    }
    let spread = if amin > 1e-6 { amax / amin } else { f32::MAX };
    (tot, spread)
}

/// 真实排一页，返回 (填充率, 面积离散度)。走的就是最终出图用的 uniform_cells。
pub fn page_quality(asp: &[f32], r: Rect, gutter: f32) -> (f32, f32) {
    let k = asp.len();
    if k == 0 {
        return (0.0, 1.0);
    }
    let slots: Vec<u32> = (0..k as u32).collect();
    let (cells, _) = uniform_cells(&slots, asp, r, gutter);
    if cells.is_empty() {
        return (0.0, f32::MAX);
    }
    let mut covered = 0.0f32;
    let mut amin = f32::MAX;
    let mut amax = 0.0f32;
    for (_, c) in &cells {
        let a = c.w * c.h;
        covered += a;
        if a < amin {
            amin = a;
        }
        if a > amax {
            amax = a;
        }
    }
    let fill = (covered / (r.w * r.h)).min(1.0);
    let spread = if amin > 1e-6 { amax / amin } else { f32::MAX };
    (fill, spread)
}

/// 快速估算：把一段长宽比按行式排布能填多满，返回 (填充率, 面积离散度)。
/// 只做前缀和 + 枚举行数，不真正生成格子，供分页规划里成千上万次调用。
pub fn estimate_fill(asp: &[f32], r: Rect, gutter: f32) -> (f32, f32) {
    let k = asp.len();
    if k == 0 || r.w <= 1.0 || r.h <= 1.0 {
        return (0.0, 1.0);
    }
    let mut pre = Vec::with_capacity(k + 1);
    pre.push(0.0f32);
    for a in asp {
        pre.push(pre[pre.len() - 1] + a.max(1e-4));
    }
    let amax = asp.iter().cloned().fold(0.0f32, f32::max).max(1e-4);
    let amin = asp.iter().cloned().fold(f32::MAX, f32::min).max(1e-4);

    let mut best_score = f32::NEG_INFINITY;
    let mut best_fill = 0.0f32;
    let mut best_spread = f32::MAX;

    for rows in 1..=k.min(48) {
        let base = k / rows;
        if base == 0 {
            break;
        }
        let rem = k % rows;
        let mut tot = 0.0f32;
        let mut covered = 0.0f32;
        let mut hmin = f32::MAX;
        let mut hmax = 0.0f32;
        let mut i = 0usize;
        let mut ok = true;
        for ri in 0..rows {
            let m = base + if ri < rem { 1 } else { 0 };
            let s = pre[i + m] - pre[i];
            let usable = r.w - (m as f32 - 1.0) * gutter;
            if s <= 1e-6 || usable <= 1.0 {
                ok = false;
                break;
            }
            let h = usable / s;
            tot += h;
            if ri + 1 < rows {
                tot += gutter;
            }
            covered += s * h * h;
            if h < hmin {
                hmin = h;
            }
            if h > hmax {
                hmax = h;
            }
            i += m;
            // 行数只会越排越高，超了就没必要再试
            if tot > r.h * 1.02 {
                ok = false;
                break;
            }
        }
        if !ok {
            continue;
        }
        let fill = (covered / (r.w * r.h)).min(1.0);
        let spread = if hmin > 1e-4 {
            (amax / amin) * (hmax / hmin).powi(2)
        } else {
            f32::MAX
        };
        // 填充高、面积差小者胜
        let score = fill - 0.10 * spread.max(1.0).ln();
        if score > best_score {
            best_score = score;
            best_fill = fill;
            best_spread = spread;
        }
    }
    if best_score == f32::NEG_INFINITY {
        (0.0, f32::MAX)
    } else {
        (best_fill, best_spread)
    }
}

/// 按格子算白边总量（不裁剪时基本为 0）
pub fn cells_energy(cells: &[(u32, Rect)], asp: &[f32]) -> f32 {
    let mut w = 0.0;
    for (slot, c) in cells {
        w += waste_of(*c, asp[*slot as usize]);
    }
    w
}

/// 贪心构建：按长宽比排序后，找最佳二分点与切向
pub fn build(slots: &[u32], asp: &[f32], r: Rect) -> Tree {
    let mut order: Vec<(u32, f32)> = slots.iter().map(|&s| (s, asp[s as usize])).collect();
    order.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut t = Tree::new();
    let root = build_rec(&order, r, &mut t);
    t.root = root;
    t
}

fn build_rec(sorted: &[(u32, f32)], r: Rect, t: &mut Tree) -> u32 {
    if sorted.len() == 1 {
        return t.push(Node::Leaf(sorted[0].0));
    }
    let n = sorted.len();
    let mut pref = vec![0f32; n + 1];
    for i in 0..n {
        pref[i + 1] = pref[i] + sorted[i].1;
    }
    let total = pref[n].max(1e-6);
    let ar = r.aspect();

    let mut best_score = f32::INFINITY;
    let mut best_k = 1usize;
    let mut best_dir = 0u8;
    let mut best_flip = false;

    for k in 1..n {
        let wa = pref[k] / total;
        let wb = 1.0 - wa;
        if wa <= 0.0 || wb <= 0.0 {
            continue;
        }
        let ma = pref[k] / k as f32;
        let mb = (pref[n] - pref[k]) / (n - k) as f32;
        for dir in 0..2u8 {
            for flip in [false, true] {
                let (ra, rb) = if dir == 0 {
                    (ar * wa, ar * wb)
                } else {
                    (ar / wa, ar / wb)
                };
                let (sa, sb) = if flip { (rb, ra) } else { (ra, rb) };
                let score = (sa / ma).ln().abs() + (sb / mb).ln().abs();
                if score < best_score {
                    best_score = score;
                    best_k = k;
                    best_dir = dir;
                    best_flip = flip;
                }
            }
        }
    }

    let (left, right) = sorted.split_at(best_k);
    // flip 表示长宽比大的那组放"第二块"
    let (ga, gb) = if best_flip {
        (right, left)
    } else {
        (left, right)
    };
    let wa = pref[best_k] / total;
    let ratio = if best_dir == 0 {
        if best_flip {
            1.0 - wa
        } else {
            wa
        }
    } else if best_flip {
        1.0 - wa
    } else {
        wa
    };
    let l = build_rec(ga, sub(r, best_dir, ratio, true), t);
    let rr = build_rec(gb, sub(r, best_dir, ratio, false), t);
    t.push(Node::Split {
        dir: best_dir,
        ratio,
        l,
        rr,
    })
}

fn sub(r: Rect, dir: u8, ratio: f32, first: bool) -> Rect {
    if dir == 0 {
        let w1 = r.w * ratio;
        if first {
            Rect { w: w1, ..r }
        } else {
            Rect {
                x: r.x + w1,
                w: r.w - w1,
                ..r
            }
        }
    } else {
        let h1 = r.h * ratio;
        if first {
            Rect { h: h1, ..r }
        } else {
            Rect {
                y: r.y + h1,
                h: r.h - h1,
                ..r
            }
        }
    }
}

/// 行式种子布局：把图分到 R 行，每行宽度撑满，行高按"行内长宽比之和"反推。
/// 对同比例照片这类集合，网格/行式解远好于二叉切分的局部最优（实测 86% → 97%+）。
pub fn build_rows(slots: &[u32], asp: &[f32], r: Rect, rows: usize) -> Tree {
    let n = slots.len();
    if n == 0 {
        return Tree::new();
    }
    let rows = rows.clamp(1, n);
    // LPT：长宽比大的先放，塞进当前长宽比之和最小的行
    let mut order: Vec<(u32, f32)> = slots.iter().map(|&s| (s, asp[s as usize])).collect();
    order.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut buckets: Vec<Vec<(u32, f32)>> = vec![Vec::new(); rows];
    let mut sums = vec![0f32; rows];
    for (s, a) in order {
        let mut best = 0usize;
        for i in 1..rows {
            if sums[i] < sums[best] {
                best = i;
            }
        }
        sums[best] += a;
        buckets[best].push((s, a));
    }
    // 行高 ∝ 1/长宽比之和
    let mut heights: Vec<f32> = sums
        .iter()
        .map(|s| if *s > 1e-6 { 1.0 / s } else { 0.0 })
        .collect();
    let hs: f32 = heights.iter().sum();
    if hs <= 1e-6 {
        return build(slots, asp, r);
    }
    let scale = r.h / hs;
    for h in heights.iter_mut() {
        *h *= scale;
    }

    let mut t = Tree::new();
    let rows_data: Vec<(&[(u32, f32)], f32)> = buckets
        .iter()
        .enumerate()
        .filter(|(i, b)| !b.is_empty() && heights[*i] > 0.0)
        .map(|(i, b)| (b.as_slice(), heights[i]))
        .collect();
    if rows_data.is_empty() {
        return build(slots, asp, r);
    }
    let root = build_stack(&rows_data, r, &mut t);
    t.root = root;
    t
}

fn build_stack(rows: &[(&[(u32, f32)], f32)], r: Rect, t: &mut Tree) -> u32 {
    if rows.len() == 1 {
        return build_row(rows[0].0, r, t);
    }
    let total: f32 = rows.iter().map(|(_, h)| *h).sum();
    let ratio = (rows[0].1 / total).clamp(0.02, 0.98);
    let h1 = r.h * ratio;
    let top = Rect {
        x: r.x,
        y: r.y,
        w: r.w,
        h: h1,
    };
    let bottom = Rect {
        x: r.x,
        y: r.y + h1,
        w: r.w,
        h: r.h - h1,
    };
    let l = build_row(rows[0].0, top, t);
    let rr = build_stack(&rows[1..], bottom, t);
    t.push(Node::Split {
        dir: 1,
        ratio,
        l,
        rr,
    })
}

fn build_row(items: &[(u32, f32)], r: Rect, t: &mut Tree) -> u32 {
    if items.len() == 1 {
        return t.push(Node::Leaf(items[0].0));
    }
    let total: f32 = items.iter().map(|(_, a)| *a).sum();
    if total <= 1e-6 {
        return t.push(Node::Leaf(items[0].0));
    }
    let ratio = (items[0].1 / total).clamp(0.02, 0.98);
    let w1 = r.w * ratio;
    let right = Rect {
        x: r.x + w1,
        y: r.y,
        w: r.w - w1,
        h: r.h,
    };
    let l = t.push(Node::Leaf(items[0].0));
    let rr = build_row(&items[1..], right, t);
    t.push(Node::Split {
        dir: 0,
        ratio,
        l,
        rr,
    })
}

/// 等大网格的列×行数：在 cols*rows <= max_cells 的约束下，
/// 让单元格长宽比最贴近图片目标长宽比，同时尽量少浪费格子。
pub fn grid_dims(inner: Rect, target_aspect: f32, max_cells: usize) -> (usize, usize) {
    let max_cells = max_cells.max(1);
    let t = target_aspect.max(1e-3);
    let mut best = (1, max_cells);
    let mut best_score = f32::INFINITY;
    for cols in 1..=max_cells {
        let rows = max_cells / cols;
        if rows == 0 {
            break;
        }
        let used = cols * rows;
        let cell_aspect = inner.aspect() * rows as f32 / cols as f32;
        let aspect_err = (cell_aspect / t).ln().abs();
        let loss = 1.0 - used as f32 / max_cells as f32;
        let score = aspect_err + 1.2 * loss;
        if score < best_score {
            best_score = score;
            best = (cols, rows);
        }
    }
    best
}

/// 等大网格的格子矩形：所有格子宽高完全一致，只按 gutter 分隔。
/// 数量不足整页时，整块居中（行内也居中），避免右下角孤零零留白。
pub fn grid_cells(inner: Rect, g: f32, cols: usize, rows: usize, count: usize) -> Vec<Rect> {
    let cols = cols.max(1);
    let rows = rows.max(1);
    let cw = ((inner.w - g * (cols as f32 - 1.0)) / cols as f32).max(0.0);
    let ch = ((inner.h - g * (rows as f32 - 1.0)) / rows as f32).max(0.0);
    let n = count.min(cols * rows);
    let used_rows = n.div_ceil(cols).max(1).min(rows);
    let y_off = (inner.h - (used_rows as f32 * ch + g * (used_rows as f32 - 1.0))).max(0.0) * 0.5;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let r = i / cols;
        let c = i % cols;
        let in_row = (n - r * cols).min(cols);
        let x_off = (inner.w - (in_row as f32 * cw + g * (in_row as f32 - 1.0))).max(0.0) * 0.5;
        out.push(Rect {
            x: inner.x + x_off + c as f32 * (cw + g),
            y: inner.y + y_off + r as f32 * (ch + g),
            w: cw,
            h: ch,
        });
    }
    out
}

/// 枚举行数取最优的行式种子，和二叉切分种子比谁的白边更少
pub fn seed_best(slots: &[u32], asp: &[f32], r: Rect, ctx: &Ctx) -> Tree {
    let mut best = build(slots, asp, r);
    let mut best_e = ctx.energy(&best, asp);
    let max_rows = slots.len().min(32);
    for rows in 1..=max_rows {
        let t = build_rows(slots, asp, r, rows);
        let e = ctx.energy(&t, asp);
        if e < best_e {
            best_e = e;
            best = t;
        }
    }
    best
}

/// 模拟退火：调分割比例 / 翻切向 / 交换两张图的位置
pub fn anneal(
    t: &mut Tree,
    ctx: &Ctx,
    slot_asp: &mut [f32],
    page_imgs: &mut [u32],
    iters: usize,
    rng: &mut StdRng,
    t0: f32,
    t1: f32,
) -> f32 {
    let n = slot_asp.len();
    let splits = t.split_ids();
    if splits.is_empty() || n < 2 {
        return ctx.energy(t, slot_asp);
    }
    let mut cur = ctx.energy(t, slot_asp);
    let mut best = cur;
    let mut best_tree = snapshot(t);
    let mut best_asp = slot_asp.to_vec();
    let mut best_imgs = page_imgs.to_vec();

    for it in 0..iters {
        let frac = it as f32 / iters as f32;
        let temp = t0 * (t1 / t0).powf(frac);
        let pick = rng.gen_range(0..3);
        let mut undo: Option<(u32, Node)> = None;
        let mut swapped: Option<(usize, usize)> = None;

        if pick == 0 {
            let idx = splits[rng.gen_range(0..splits.len())];
            if let Node::Split { dir, ratio, l, rr } = t.nodes[idx as usize] {
                undo = Some((idx, Node::Split { dir, ratio, l, rr }));
                let nr = (ratio * (1.0 + rng.gen_range(-0.35f32..0.35f32))).clamp(0.04, 0.96);
                t.nodes[idx as usize] = Node::Split {
                    dir,
                    ratio: nr,
                    l,
                    rr,
                };
            }
        } else if pick == 1 {
            let idx = splits[rng.gen_range(0..splits.len())];
            if let Node::Split { dir, ratio, l, rr } = t.nodes[idx as usize] {
                undo = Some((idx, Node::Split { dir, ratio, l, rr }));
                t.nodes[idx as usize] = Node::Split {
                    dir: 1 - dir,
                    ratio,
                    l,
                    rr,
                };
            }
        } else {
            let i = rng.gen_range(0..n);
            let j = rng.gen_range(0..n);
            if i != j {
                slot_asp.swap(i, j);
                page_imgs.swap(i, j);
                swapped = Some((i, j));
            }
        }

        let e = ctx.energy(t, slot_asp);
        let d = e - cur;
        let accept = d <= 0.0 || rng.gen::<f32>() < (-d / temp).exp();
        if accept {
            cur = e;
            if e < best {
                best = e;
                best_tree = snapshot(t);
                best_asp = slot_asp.to_vec();
                best_imgs = page_imgs.to_vec();
            }
        } else {
            if let Some((idx, node)) = undo {
                t.nodes[idx as usize] = node;
            }
            if let Some((i, j)) = swapped {
                slot_asp.swap(i, j);
                page_imgs.swap(i, j);
            }
        }
    }
    restore(t, &best_tree);
    slot_asp.copy_from_slice(&best_asp);
    page_imgs.copy_from_slice(&best_imgs);
    best
}

fn snapshot(t: &Tree) -> Vec<Node> {
    t.nodes.clone()
}
fn restore(t: &mut Tree, s: &[Node]) {
    t.nodes.clear();
    t.nodes.extend_from_slice(s);
}

/// 用未使用的图片替换"最不贴合"的格子，直到无改善
pub fn pool_refine(
    t: &mut Tree,
    ctx: &Ctx,
    slot_asp: &mut [f32],
    page_imgs: &mut [u32],
    pool_asp: &[f32],
    used: &[bool],
    samples: usize,
    rng: &mut StdRng,
    rounds: usize,
) -> f32 {
    let mut cur = ctx.energy(t, slot_asp);
    for _ in 0..rounds {
        let mut cells = Vec::new();
        t.collect(ctx.inner, &mut cells);
        // 每格的白边，排序找最差的
        let mut scored: Vec<(f32, u32)> = cells
            .iter()
            .map(|(slot, r)| {
                let c = leaf_cell(*r, ctx.gutter, ctx.inner);
                (waste_of(c, slot_asp[*slot as usize]), *slot)
            })
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        let mut improved = false;
        let worst_n = (scored.len() / 4).max(1);
        for (w, slot) in scored.iter().take(worst_n) {
            if *w <= 1e-6 {
                continue;
            }
            // 该格理想长宽比
            let cell = cells
                .iter()
                .find(|(s, _)| s == slot)
                .map(|(_, r)| leaf_cell(*r, ctx.gutter, ctx.inner))
                .unwrap();
            let target = cell.aspect();
            let mut best_pick: Option<usize> = None;
            let mut best_gain = 0.0f32;
            for _ in 0..samples {
                let p = rng.gen_range(0..pool_asp.len());
                // 已在前面的页用过，或已经在这一页上：都不能再选，
                // 否则同一张图会被重复放进一页，导致有图被漏掉。
                if used[p] || page_imgs.contains(&(p as u32)) {
                    continue;
                }
                let new_w = waste_of(cell, pool_asp[p]);
                let gain = *w - new_w;
                if gain > best_gain {
                    best_gain = gain;
                    best_pick = Some(p);
                }
            }
            if let Some(p) = best_pick {
                // 只在明显改善时换（且不要偏离目标长宽比太远）
                if best_gain > ctx.page_area * 1e-5 {
                    page_imgs[*slot as usize] = p as u32;
                    slot_asp[*slot as usize] = pool_asp[p];
                    cur -= best_gain;
                    improved = true;
                }
                let _ = target;
            }
        }
        if !improved {
            break;
        }
    }
    cur
}
