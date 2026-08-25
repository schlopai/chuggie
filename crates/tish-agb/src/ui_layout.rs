//! Native flex layout solver for `packages/ui.tish`.
//!
//! The UI's measure/arrange passes were the largest block of boxed tish left in the engine: ~190
//! lines walking a pool of `LNode` records through `Value` property reads. Every field touch was a
//! string-keyed lookup on a boxed object, and the pass runs over every node of every screen — the
//! file's own notes measure it at roughly 2,000 ticks PER NODE, so a 60-node tab spent ~27 frames
//! laying out before a glyph was drawn, and both passes had to interleave `audio_pump()` calls just
//! to keep the music from stuttering through them.
//!
//! `LNode` is already all-`i32` (22 numeric fields, no strings), so the pass ports to a flat pool
//! with no representation problem: one `Vec<Node>` of plain integers, two passes of ordinary Rust
//! arithmetic, zero `Value`s. This is the same shape the tish side already flattens into — the
//! translation is field-for-field, deliberately, so the two can be diffed.
//!
//! ⚠️ SEMANTICS ARE PINNED TO THE TISH VERSION, including its integer division (`leftover / 2`
//! truncates toward zero, and `leftover * g / totalGrow` multiplies BEFORE dividing). Do not
//! "clean up" the arithmetic: the boxed path and this one must produce identical pixels, and the
//! ordering is what makes the rounding match.
//!
//! Extends tish through the `cargo:` mechanism — no compiler change. See `tish.d.tish`.

extern crate alloc;
use alloc::vec::Vec;

/// One layout node. Field-for-field with `interface LNode` in packages/ui.tish.
#[derive(Clone, Copy, Default)]
pub struct Node {
    pub kind: i32, // 0 = container, non-zero = leaf (text/icon/spacer)
    pub dir: i32,  // 1 = row, 0 = column
    pub gap: i32,
    pub pad: i32,
    pub fw: i32, // fixed width,  < 0 = unset
    pub fh: i32, // fixed height, < 0 = unset
    pub grow: i32,
    pub am: i32,     // align:   0 start, 1 center, 2 end, 3 stretch
    pub jm: i32,     // justify: 0 start, 1 center, 2 end, 3 between
    pub scroll: i32, // non-zero = scroll container
    pub sy: i32,     // scroll offset
    #[allow(dead_code)] // mirrors the script-side node layout; kept for parity
    pub parent: i32,
    pub fc: i32,   // first child, -1 = none
    pub ns: i32,   // next sibling, -1 = none
    pub last: i32, // last child, -1 = none
    pub mw: i32,   // measured width
    pub mh: i32,   // measured height
    pub x: i32,
    pub y: i32,
    pub cw: i32, // computed width
    pub ch: i32, // computed height
    pub hide: i32,
    // Content/viewport extent of a scroll container, for the scrollbar. The tish version wrote
    // these onto the RAW boxed node instead of onto every LNode; here they are two more i32s in a
    // struct that is already 22 wide, and the caller reads them back for the one or two scroll
    // containers a screen has.
    pub content: i32,
    pub view: i32,

    // ── paint state ──────────────────────────────────────────────────────────────────────────────
    // Resolved ONCE by the caller (at flatten, where the node is already open) so the paint pass
    // needs nothing from the tish side. Colours are already-resolved RGB, not theme keys; `align`
    // is an id, not a string; `use_w` says whether the alignment wants the laid-out width.
    pub paint_kind: i32, // 0 = container/none, 1 = text, 2 = icon, 3 = custom (tish paints it)
    pub col: i32,
    pub shadowc: i32,
    pub fillc: i32,
    pub borderc: i32,
    pub font: i32,
    pub align: i32, // 0 left, 1 centre, 2 right
    pub use_w: i32,
    pub shadow_off: i32,
    pub sel: i32,
}

/// The pool. Persists between renders exactly like the tish `LN` pool: `reset` truncates the live
/// count without freeing, so a screen of the same shape re-lays out with no allocation.
///
/// `text` is parallel to `nodes` and holds each text leaf's string. It is uploaded once per screen
/// (or whenever a label changes), which is what lets the paint pass run with NO boundary crossings:
/// the alternative — reading `raw.text` per node per frame — is the cost this whole file exists to
/// remove. Strings are kept, not cleared, on `reset`, so a re-render of the same screen re-uses the
/// allocation.
pub struct Pool {
    pub nodes: Vec<Node>,
    pub count: usize,
    pub text: Vec<alloc::string::String>,
}

impl Pool {
    pub const fn new() -> Self {
        Pool {
            nodes: Vec::new(),
            count: 0,
            text: Vec::new(),
        }
    }

    pub fn reset(&mut self) {
        self.count = 0;
    }

    /// Append a node, reusing a pooled slot when one exists. Returns its index.
    #[allow(clippy::too_many_arguments)]
    pub fn push(
        &mut self,
        kind: i32,
        dir: i32,
        gap: i32,
        pad: i32,
        fw: i32,
        fh: i32,
        grow: i32,
        am: i32,
        jm: i32,
        scroll: i32,
        sy: i32,
        parent: i32,
    ) -> i32 {
        let idx = self.count;
        if idx == self.nodes.len() {
            self.nodes.push(Node::default());
        }
        self.count += 1;
        let n = &mut self.nodes[idx];
        *n = Node {
            kind,
            dir,
            gap,
            pad,
            fw,
            fh,
            grow,
            am,
            jm,
            scroll,
            sy,
            parent,
            fc: -1,
            ns: -1,
            last: -1,
            mw: 0,
            mh: 0,
            x: 0,
            y: 0,
            cw: 0,
            ch: 0,
            hide: 0,
            content: 0,
            view: 0,
            paint_kind: 0,
            col: 0xFFFFFF,
            shadowc: -1,
            fillc: -1,
            borderc: -1,
            font: -1,
            align: 0,
            use_w: 0,
            shadow_off: 1,
            sel: 0,
        };
        // Link into the parent's child list, keeping source order (append at `last`).
        if parent >= 0 {
            let p = parent as usize;
            let lc = self.nodes[p].last;
            if lc < 0 {
                self.nodes[p].fc = idx as i32;
            } else {
                self.nodes[lc as usize].ns = idx as i32;
            }
            self.nodes[p].last = idx as i32;
        }
        idx as i32
    }

    /// Everything paint needs for one node, resolved by the caller.
    #[allow(clippy::too_many_arguments)]
    pub fn set_paint(
        &mut self,
        i: i32,
        paint_kind: i32,
        col: i32,
        shadowc: i32,
        fillc: i32,
        borderc: i32,
        font: i32,
        align: i32,
        use_w: i32,
        shadow_off: i32,
        sel: i32,
    ) {
        if let Some(n) = self.nodes.get_mut(i as usize) {
            n.paint_kind = paint_kind;
            n.col = col;
            n.shadowc = shadowc;
            n.fillc = fillc;
            n.borderc = borderc;
            n.font = font;
            n.align = align;
            n.use_w = use_w;
            n.shadow_off = shadow_off;
            n.sel = sel;
        }
    }

    /// A text leaf's string. Uploaded once per screen; call again when a label changes.
    pub fn set_text(&mut self, i: i32, s: &str) {
        let i = i as usize;
        while self.text.len() <= i {
            self.text.push(alloc::string::String::new());
        }
        self.text[i].clear();
        self.text[i].push_str(s);
    }

    /// A leaf's measured size, set by the caller (text/icon extents come from the font, which lives
    /// on the tish side). Containers compute their own in [`Self::solve`].
    pub fn set_measured(&mut self, i: i32, mw: i32, mh: i32) {
        if let Some(n) = self.nodes.get_mut(i as usize) {
            n.mw = mw;
            n.mh = mh;
        }
    }

    /// Both passes. `root_*` is the box node 0 is laid into (the full screen, or a sub-rect for a
    /// panel swap — the tish `LAYOUT.sub*` path).
    pub fn solve(&mut self, root_x: i32, root_y: i32, root_w: i32, root_h: i32) {
        let count = self.count;
        if count == 0 {
            return;
        }

        // ── Pass 1: MEASURE (bottom-up). Children precede parents in the flat order, so one
        // backward scan settles every container without recursion.
        let mut i = count as i32 - 1;
        while i >= 0 {
            let idx = i as usize;
            if self.nodes[idx].kind == 0 {
                let is_row = self.nodes[idx].dir;
                let mut sum_main = 0i32;
                let mut max_cross = 0i32;
                let mut cnt = 0i32;
                let mut c = self.nodes[idx].fc;
                while c >= 0 {
                    let ch = &self.nodes[c as usize];
                    let cwn = if ch.fw < 0 { ch.mw } else { ch.fw };
                    let chn = if ch.fh < 0 { ch.mh } else { ch.fh };
                    let (cm, cc) = if is_row > 0 { (cwn, chn) } else { (chn, cwn) };
                    sum_main += cm;
                    if cc > max_cross {
                        max_cross = cc;
                    }
                    cnt += 1;
                    c = ch.ns;
                }
                let gap = self.nodes[idx].gap;
                if cnt > 1 {
                    sum_main += gap * (cnt - 1);
                }
                let pad = self.nodes[idx].pad;
                let main_t = sum_main + pad * 2;
                let cross_t = max_cross + pad * 2;
                let n = &mut self.nodes[idx];
                if is_row > 0 {
                    n.mw = main_t;
                    n.mh = cross_t;
                } else {
                    n.mw = cross_t;
                    n.mh = main_t;
                }
            }
            i -= 1;
        }

        // ── Pass 2: ARRANGE (top-down). A container's own box is already set by the time the
        // forward scan reaches it, so this is one pass too.
        {
            let r = &mut self.nodes[0];
            r.x = root_x;
            r.y = root_y;
            r.cw = root_w;
            r.ch = root_h;
        }
        let mut i = 0usize;
        while i < count {
            if self.nodes[i].kind == 0 {
                let me = self.nodes[i];
                let is_row = me.dir;
                let pad = me.pad;
                let gap = me.gap;
                let am = me.am;
                let jm = me.jm;
                let scroll = me.scroll;
                let ix = me.x + pad;
                let iy = me.y + pad;
                let iw = me.cw - pad * 2;
                let ih = me.ch - pad * 2;
                let (main_len, mut cross_len) = if is_row > 0 { (iw, ih) } else { (ih, iw) };

                let mut sum_main = 0i32;
                let mut total_grow = 0i32;
                let mut cnt = 0i32;
                let mut c = me.fc;
                while c >= 0 {
                    let ch = &self.nodes[c as usize];
                    let cwn = if ch.fw < 0 { ch.mw } else { ch.fw };
                    let chn = if ch.fh < 0 { ch.mh } else { ch.fh };
                    sum_main += if is_row > 0 { cwn } else { chn };
                    total_grow += ch.grow;
                    cnt += 1;
                    c = ch.ns;
                }
                let gapsum = if cnt > 1 { gap * (cnt - 1) } else { 0 };
                let mut leftover = main_len - sum_main - gapsum;
                if leftover < 0 {
                    leftover = 0;
                }

                if scroll > 0 {
                    let n = &mut self.nodes[i];
                    n.content = sum_main + gapsum;
                    n.view = main_len;
                    // Column scroll reserves the right edge for the bar, as the tish path does.
                    if is_row == 0 && sum_main + gapsum > main_len && cross_len > SB_GUTTER {
                        cross_len -= SB_GUTTER;
                    }
                }

                let mut off = 0i32;
                let mut egap = gap;
                if total_grow <= 0 {
                    if jm == 1 {
                        off = leftover / 2;
                    }
                    if jm == 2 {
                        off = leftover;
                    }
                    if jm == 3 && cnt > 1 {
                        egap = gap + leftover / (cnt - 1);
                    }
                }

                let mut run_pos = off - me.sy;
                let phide = me.hide;
                let mut c = me.fc;
                while c >= 0 {
                    let ci = c as usize;
                    let ch = self.nodes[ci];
                    let cwn = if ch.fw < 0 { ch.mw } else { ch.fw };
                    let chn = if ch.fh < 0 { ch.mh } else { ch.fh };
                    let (mut cm, mut cc) = if is_row > 0 { (cwn, chn) } else { (chn, cwn) };
                    if total_grow > 0 && ch.grow > 0 {
                        // ⚠️ multiply BEFORE divide, matching the tish arithmetic exactly.
                        cm += leftover * ch.grow / total_grow;
                    }
                    if am == 3 {
                        // Stretch only when the child set no fixed cross size.
                        let unset = if is_row > 0 { ch.fh < 0 } else { ch.fw < 0 };
                        if unset {
                            cc = cross_len;
                        }
                    }
                    let mut cpos = 0i32;
                    if am == 1 {
                        cpos = (cross_len - cc) / 2;
                    }
                    if am == 2 {
                        cpos = cross_len - cc;
                    }
                    // Cull a scroll child that is not FULLY inside the viewport — there is no pixel
                    // clipping, so a half-scrolled row would spill past the edge.
                    let mut hide = phide;
                    if scroll > 0 && (run_pos < 0 || run_pos + cm > main_len) {
                        hide = 1;
                    }
                    let n = &mut self.nodes[ci];
                    n.hide = hide;
                    if is_row > 0 {
                        n.x = ix + run_pos;
                        n.y = iy + cpos;
                        n.cw = cm;
                        n.ch = cc;
                    } else {
                        n.x = ix + cpos;
                        n.y = iy + run_pos;
                        n.cw = cc;
                        n.ch = cm;
                    }
                    run_pos += cm + egap;
                    c = ch.ns;
                }
            }
            i += 1;
        }
    }
}

/// Gutter reserved for a column scrollbar. Mirrors `SB_GUTTER` in packages/ui.tish.
const SB_GUTTER: i32 = 6;

#[cfg(test)]
mod tests {
    use super::*;

    /// A row of three fixed children inside a padded container: positions must step by width+gap,
    /// and the container must measure to content+padding.
    #[test]
    fn row_of_three_with_gap_and_pad() {
        let mut p = Pool::new();
        let root = p.push(0, 1, 4, 2, -1, -1, 0, 0, 0, 0, 0, -1);
        for _ in 0..3 {
            p.push(1, 0, 0, 0, 10, 8, 0, 0, 0, 0, 0, root);
        }
        p.solve(0, 0, 240, 160);
        // 3*10 + 2*4 gap + 2*2 pad
        assert_eq!(p.nodes[0].mw, 30 + 8 + 4);
        assert_eq!(p.nodes[1].x, 2);
        assert_eq!(p.nodes[2].x, 2 + 10 + 4);
        assert_eq!(p.nodes[3].x, 2 + 20 + 8);
        assert_eq!(p.nodes[1].y, 2);
    }

    /// `grow` splits the leftover main-axis space; the arithmetic must match tish's
    /// multiply-then-divide truncation.
    #[test]
    fn grow_splits_leftover() {
        let mut p = Pool::new();
        let root = p.push(0, 1, 0, 0, -1, -1, 0, 0, 0, 0, 0, -1);
        p.push(1, 0, 0, 0, 10, 8, 1, 0, 0, 0, 0, root);
        p.push(1, 0, 0, 0, 10, 8, 1, 0, 0, 0, 0, root);
        p.solve(0, 0, 100, 50);
        // leftover 80, split 40/40
        assert_eq!(p.nodes[1].cw, 50);
        assert_eq!(p.nodes[2].cw, 50);
        assert_eq!(p.nodes[2].x, 50);
    }

    /// justify=center offsets the run by half the leftover (truncating).
    #[test]
    fn justify_center_truncates() {
        let mut p = Pool::new();
        let root = p.push(0, 1, 0, 0, -1, -1, 0, 0, 1, 0, 0, -1);
        p.push(1, 0, 0, 0, 10, 8, 0, 0, 0, 0, 0, root);
        p.solve(0, 0, 25, 50);
        assert_eq!(p.nodes[1].x, (25 - 10) / 2);
    }

    /// A scroll container culls any child not fully inside the viewport, and records content/view.
    #[test]
    fn scroll_culls_partial_children() {
        let mut p = Pool::new();
        let root = p.push(0, 0, 0, 0, -1, -1, 0, 0, 0, 1, 0, -1);
        for _ in 0..4 {
            p.push(1, 0, 0, 0, 10, 10, 0, 0, 0, 0, 0, root);
        }
        p.solve(0, 0, 40, 25);
        assert_eq!(p.nodes[0].content, 40);
        assert_eq!(p.nodes[0].view, 25);
        assert_eq!(p.nodes[1].hide, 0); // 0..10 inside
        assert_eq!(p.nodes[3].hide, 1); // 20..30 spills past 25
    }

    /// The pool reuses slots across renders instead of allocating per screen.
    #[test]
    fn reset_reuses_slots() {
        let mut p = Pool::new();
        let root = p.push(0, 1, 0, 0, -1, -1, 0, 0, 0, 0, 0, -1);
        p.push(1, 0, 0, 0, 4, 4, 0, 0, 0, 0, 0, root);
        let cap = p.nodes.len();
        p.reset();
        assert_eq!(p.count, 0);
        let root2 = p.push(0, 1, 0, 0, -1, -1, 0, 0, 0, 0, 0, -1);
        p.push(1, 0, 0, 0, 4, 4, 0, 0, 0, 0, 0, root2);
        assert_eq!(p.nodes.len(), cap, "re-render must not grow the pool");
        assert_eq!(p.nodes[0].fc, 1, "links must be rebuilt, not inherited");
    }
}
