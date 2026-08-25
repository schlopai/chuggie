#!/usr/bin/env python3
"""Shared art baking for the character-driven genres — `examples/versus` and `examples/beatemup`.

Both games are built from the same CC0 LuizMelo packs and the same fixed 24-pose sheet layout, so
the pipeline lives here rather than being copy-pasted: the four traps below are ones that look fine
in every preview and only fail on hardware, and they should be fixable in one place.

  1. THE PACKS ARE DRAWN AT FOUR DIFFERENT SCALES. Idle body heights are 52 / 56 / 41 / 81 px.
     Dropped in side by side that reads as a rendering bug, so every character is resampled to a
     common height. NEAREST for upscales (a blurred 4bpp sprite quantises horribly), LANCZOS for
     downscales.

  2. ALPHA MUST BE HARDENED TO 0 OR 255. A GBA sprite has one transparent colour, not an alpha
     channel — see `harden_alpha`.

  3. AN ATTACK FRAME IS BIGGER THAN ANY GBA SPRITE, IN BOTH AXES, so each one is cut into the body
     cell plus ONE ADJACENT 64x64 window — see `build_char`.

  4. QUANTISE THE ASSEMBLED SHEET, NEVER PER FRAME. `include_aseprite_inner!` emits one Palette16
     per PNG, so body + FX + portrait in one file costs ONE of the GBA's 16 sprite palette banks.
     Quantising twice produces two palettes.
"""
import os

from PIL import Image

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
ART = os.path.expanduser("~/Downloads/versus-art")

CELL = 64          # sheet64: — body and FX cells alike
FEET = 62          # the row inside a cell that the character's feet sit on

# The 24-pose body layout, identical for every character and both games — this is what lets one
# frame-data table describe a whole roster. (clip, "pick" spec); see pick().
POSES = [
    ("idle", 0.00), ("idle", 0.25), ("idle", 0.50), ("idle", 0.75),      # 0-3   idle
    ("walk", 0.00), ("walk", 0.25), ("walk", 0.50), ("walk", 0.75),      # 4-7   walk
    ("jump", 0.00),                                                       # 8     jump
    ("fall", 1.00),                                                       # 9     fall
    ("crouch", 0.50),                                                     # 10    crouch
    ("atk1", -1.0),                                                       # 11    crouching / low attack
    ("atk1", 0.20), ("atk1", 0.55), ("atk1", 0.85),                      # 12-14 light attack
    ("atk2", 0.20), ("atk2", 0.55), ("atk2", 0.85),                      # 15-17 heavy attack
    ("atk3", 0.30), ("atk3", 0.60), ("atk3", 0.95),                      # 18-20 special
    ("atk2", 0.00),                                                       # 21    guard
    ("hit", 0.50),                                                        # 22    take hit
    ("death", 1.00),                                                      # 23    KO
]
FX_FOR = list(range(11, 21))   # body poses that also get an FX overlay cell (24..33)
PORTRAIT = 34
NCELLS = PORTRAIT + 1

# The four source packs, and the clip each logical animation comes from.
PACKS = {
    "hero": dict(dir="martial-hero/Martial Hero/Sprites", cw=200,
                 clips=dict(idle="Idle.png", walk="Run.png", jump="Jump.png", fall="Fall.png",
                            atk1="Attack1.png", atk2="Attack2.png", atk3="Attack1.png",
                            hit="Take Hit.png", death="Death.png")),
    "hero2": dict(dir="martial-hero-2/Martial Hero 2/Sprites", cw=200,
                  clips=dict(idle="Idle.png", walk="Run.png", jump="Jump.png", fall="Fall.png",
                             atk1="Attack1.png", atk2="Attack2.png", atk3="Attack2.png",
                             hit="Take hit.png", death="Death.png")),
    "hero3": dict(dir="martial-hero-3/Martial Hero 3/Sprite", cw=126,
                  clips=dict(idle="Idle.png", walk="Run.png", jump="Going Up.png",
                             fall="Going Down.png", atk1="Attack1.png", atk2="Attack2.png",
                             atk3="Attack3.png", hit="Take Hit.png", death="Death.png")),
    "warrior": dict(dir="medieval-warrior-pack/Medieval Warrior Pack", cw=184,
                    dir2="medieval-warrior-pack/Medieval Warrior (Version 1.2)",
                    clips=dict(idle="Idle.png", walk="Run.png", jump="Jump.png", fall="Fall.png",
                               atk1="Attack1.png", atk2="Attack2.png", atk3="Attack3.png",
                               hit="Hit.png", death="Death.png", crouch="Crouch.png")),
}



def harden_alpha(im, cut=128):
    """Make every pixel fully opaque or fully transparent.

    ⚠️ A GBA sprite has ONE transparent colour, not an alpha channel, so the importer keeps a pixel
    or drops it — there is no halfway. Everything upstream of here produces partial alpha: PIL's text
    rendering antialiases glyph edges, and LANCZOS resampling feathers every silhouette. Left alone,
    a 1px-wide font stroke is almost entirely edge pixels, and the importer drops nearly all of them:
    the digits came out as a few disconnected dashes, and looked perfectly fine in any preview that
    composited them over a background first."""
    im = im.convert("RGBA")
    return Image.merge("RGBA", (*im.split()[:3], im.split()[3].point(lambda a: 255 if a >= cut else 0)))



def clamp_colors(im, maxc=15):
    """Keep an RGBA sprite sheet within a GBA 4bpp sprite's colour budget (15 + transparent).
    Only touches opaque pixels; transparency is preserved."""
    im = harden_alpha(im)
    px = list(im.getdata())
    opaque = set((r, g, b) for (r, g, b, a) in px if a > 8)
    if len(opaque) <= maxc:
        return im
    q = im.convert("RGB").quantize(colors=maxc, method=Image.MEDIANCUT, dither=Image.NONE)
    q = q.convert("RGB")
    out = Image.new("RGBA", im.size, (0, 0, 0, 0))
    for i, (r, g, b, a) in enumerate(px):
        if a > 8:
            x, y = i % im.width, i // im.width
            out.putpixel((x, y), q.getpixel((x, y)) + (255,))
    return out



def union_bbox(frames):
    """The alpha bounding box that contains every frame's content (for consistent alignment)."""
    box = None
    for f in frames:
        b = f.getbbox()
        if b is None:
            continue
        box = b if box is None else (min(box[0], b[0]), min(box[1], b[1]),
                                     max(box[2], b[2]), max(box[3], b[3]))
    return box



def squash(frame, k):
    """Compress a frame vertically onto its own feet, keeping the cell geometry — a crouch."""
    b = frame.getbbox()
    if b is None:
        return frame
    body = frame.crop(b)
    nh = max(1, int(round(body.height * k)))
    out = Image.new("RGBA", frame.size, (0, 0, 0, 0))
    out.paste(body.resize((body.width, nh), Image.LANCZOS), (b[0], b[3] - nh))
    return out



def pick(frames, frac):
    """`frac` is a fraction through the clip — or -1, meaning "the most COMPACT frame in it".

    The crouching attack is drawn from the same source clip as the standing one, and picking a fixed
    fraction of it hands a crouch the frame where the character is at full stretch overhead. The
    shortest frame is the one where the swing is committed and low, which is what a crouch wants."""
    if frac < 0:
        best, bh = frames[0], 10 ** 9
        for f in frames:
            b = f.getbbox()
            if b and (b[3] - b[1]) < bh:
                best, bh = f, b[3] - b[1]
        return best
    i = int(round(frac * (len(frames) - 1)))
    return frames[max(0, min(len(frames) - 1, i))]



def count_opaque(im):
    return sum(1 for _, _, _, a in im.getdata() if a > 8)



def load_clip(pack, key):
    """Split one source strip into its frames."""
    rel = pack["clips"].get(key)
    if rel is None:
        return None
    for d in (pack.get("dir2"), pack["dir"]):
        if d is None:
            continue
        p = os.path.join(ART, d, rel)
        if os.path.exists(p):
            im = Image.open(p).convert("RGBA")
            cw = pack["cw"]
            return [im.crop((i * cw, 0, (i + 1) * cw, im.height)) for i in range(im.width // cw)]
    raise SystemExit("missing source clip %s/%s — see examples/versus/assets/ATTRIBUTION.md" % (d, rel))


def build_char(name, target_h, out_png):
    """Bake one character's 35-cell sheet. Returns {pose: (dx, dy)} for the FX overlays.

    Anchor and scale come from the IDLE clip alone. Using the union of every clip would let one wild
    attack frame drag the anchor sideways and make the character slide when it changed pose.
    """
    pack = PACKS[name]
    clips = {k: load_clip(pack, k) for k in pack["clips"]}
    if clips.get("crouch") is None:
        # Only the Medieval Warrior pack ships a crouch. Squashing the idle pose onto its own feet
        # reads as one far better than any borrowed frame does — the death and take-hit poses are
        # both off-balance, so they look like a knockdown rather than ducking on purpose.
        clips["crouch"] = [squash(clips["idle"][0], 0.72)]

    ib = union_bbox(clips["idle"])
    scale = target_h / float(ib[3] - ib[1])
    if 0.9 < scale < 1.1:
        scale = 1.0
    cx = (ib[0] + ib[2]) / 2.0
    gy = float(ib[3])

    def scaled(fr):
        if scale == 1.0:
            return fr
        w, h = fr.size
        nw, nh = max(1, int(round(w * scale))), max(1, int(round(h * scale)))
        return fr.resize((nw, nh), Image.NEAREST if scale > 1 else Image.LANCZOS)

    ax, ay = int(round(cx * scale)), int(round(gy * scale))
    body_box = (ax - CELL // 2, ay - FEET, ax + CELL // 2, ay + (CELL - FEET))

    cells = [None] * NCELLS
    fx = {}
    for idx, (clip, frac) in enumerate(POSES):
        fr = scaled(pick(clips[clip], frac))
        cells[idx] = fr.crop(body_box)
        if idx in FX_FOR:
            slot = 24 + FX_FOR.index(idx)
            # ADJACENT windows only — no diagonals. A diagonal piece shares only a corner with the
            # body cell, so whatever it holds reads as a white slab floating beside the fighter
            # rather than as the rest of the sword. An edge-sharing piece always looks joined.
            best, bdx, bdy = None, 0, 0
            for dx, dy in ((1, 0), (-1, 0), (0, -1)):
                win = fr.crop((body_box[0] + dx * CELL, body_box[1] + dy * CELL,
                               body_box[0] + (dx + 1) * CELL, body_box[1] + (dy + 1) * CELL))
                n = count_opaque(win)
                if n > 24 and (best is None or n > count_opaque(best)):
                    best, bdx, bdy = win, dx, dy
            cells[slot] = best if best is not None else Image.new("RGBA", (CELL, CELL), (0, 0, 0, 0))
            fx[idx] = (bdx, bdy)

    # A 24x24 window on the head alone, doubled. Every character is already normalised with its feet
    # on row FEET, so one rectangle frames all of them — but it has to be tight: a head-and-torso
    # crop scaled up reads as a tiny full-body sprite rather than as a portrait.
    head = cells[0].crop((20, FEET - target_h, 44, FEET - target_h + 24))
    port = Image.new("RGBA", (CELL, CELL), (0, 0, 0, 0))
    port.paste(head.resize((48, 48), Image.NEAREST), (8, 10))
    cells[PORTRAIT] = port

    strip = Image.new("RGBA", (CELL * NCELLS, CELL), (0, 0, 0, 0))
    for i, c in enumerate(cells):
        strip.paste(c if c is not None else Image.new("RGBA", (CELL, CELL), (0, 0, 0, 0)), (i * CELL, 0))
    before = len(set((r, g, b) for r, g, b, a in strip.getdata() if a > 8))
    clamp_colors(strip, 15).save(out_png)
    print("  %-8s scale %.3f  idle %dpx -> %dpx  %d cells  %d -> 15 colours"
          % (name, scale, ib[3] - ib[1], target_h, NCELLS, before))
    return fx


def digits_sheet(out_png, size=15, colour=(255, 244, 214, 255)):
    """Ten 16x16 cells, 0-9.

    ⚠️ Anything that changes while the game is running is a DIGIT SPRITE, not `text_draw`. Redrawing
    an unchanged string is free, but the moment it changes it re-shapes the glyphs and allocates
    sprite VRAM — and the things that change (a clock, a combo counter, a score) change on exactly
    the busiest frames."""
    from PIL import ImageDraw, ImageFont
    font = ImageFont.truetype(os.path.join(REPO, "assets", "fonts", "kenney-high-square.ttf"), size)
    strip = Image.new("RGBA", (16 * 10, 16), (0, 0, 0, 0))
    for d in range(10):
        cell = Image.new("RGBA", (16, 16), (0, 0, 0, 0))
        dr = ImageDraw.Draw(cell)
        box = dr.textbbox((0, 0), str(d), font=font)
        dr.text(((16 - (box[2] - box[0])) // 2 - box[0], (16 - (box[3] - box[1])) // 2 - box[1]),
                str(d), font=font, fill=colour)
        strip.paste(cell, (d * 16, 0))
    clamp_colors(strip, 15).save(out_png)


def fx_side_tish(fx_by_char, names):
    """The FX_DX / FX_DY tish arrays, in sheet order."""
    xs, ys = [], []
    for n in names:
        for i in FX_FOR:
            dx, dy = fx_by_char[n].get(i, (0, 0))
            xs.append(str(dx) if dx >= 0 else "0 - %d" % -dx)
            ys.append(str(dy) if dy >= 0 else "0 - %d" % -dy)
    return ("export const FX_DX: i32[] = [" + ", ".join(xs) + "]",
            "export const FX_DY: i32[] = [" + ", ".join(ys) + "]")
