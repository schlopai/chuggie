#!/usr/bin/env python3
"""Generate Warsong Gulch assets — dirt valley CTF map + 16px class sprites.

Writes into examples/warsong/assets/:
  wsg_tiles.png / wsg_tiles.tsj  — local Tiled tileset (NA dirt/grass/wall crops, no outlines)
  wsg.tmj                        — Tiled map referencing that tileset
  wsg16.png                      — 16px class/team/flag/VFX strip
  skills16.png                   — 16px skill icons (from Ninja Adventure Skill Icon pack)

Also writes examples/warsong/src/skill_kit.tish (names + icon frame indices).

    python3 scripts/gen_wsg.py
"""

from __future__ import annotations

import json
import pathlib
from PIL import Image, ImageDraw, ImageEnhance

ROOT = pathlib.Path(__file__).resolve().parent.parent
NA = ROOT / "assets/ninja-adventure"
SKILL_ICON_DIR = NA / "Ui" / "Skill Icon"
OUT = ROOT / "examples/warsong/assets"
SRC = ROOT / "examples/warsong" / "src"

W, H = 30, 18
TILE = 16

# Tile indices in wsg_tiles.png (order = gid - 1). Cropped from Ninja Adventure —
# seamless fill tiles, never painted cell outlines (those read as a spreadsheet).
T_FLOOR0 = 0
T_FLOOR1 = 1
T_FLOOR2 = 2
T_GRASS = 3
T_MUD = 4
T_WALL0 = 5
T_WALL1 = 6
T_BASE_A = 7
T_BASE_H = 8
T_PATH = 9
T_FLAG = 10
T_PAD = 11

# Sources: (rel under Backgrounds/Tilesets, col, row)
_FLOOR = "TilesetFloor.png"
_INT = "Interior/TilesetInteriorFloor.png"


def crop_tile(rel: str, col: int, row: int) -> Image.Image:
    src = Image.open(NA / "Backgrounds" / "Tilesets" / rel).convert("RGBA")
    x, y = col * TILE, row * TILE
    return src.crop((x, y, x + TILE, y + TILE))


def tint(im: Image.Image, rgb: tuple[int, int, int], amount: float = 0.45) -> Image.Image:
    """Blend non-transparent pixels toward a faction tint."""
    out = im.copy()
    px = out.load()
    r, g, b = rgb
    for y in range(out.height):
        for x in range(out.width):
            pr, pg, pb, pa = px[x, y]
            if pa < 8:
                continue
            px[x, y] = (
                int(pr * (1 - amount) + r * amount),
                int(pg * (1 - amount) + g * amount),
                int(pb * (1 - amount) + b * amount),
                pa,
            )
    return out


def quantize_strip(im: Image.Image) -> Image.Image:
    cols = {p for p in im.getdata() if (p[3] if len(p) > 3 else 255) > 0}
    if len(cols) <= 15:
        return im
    a = im.getchannel("A") if im.mode == "RGBA" else None
    q = im.convert("RGB").quantize(colors=15, dither=Image.NONE).convert("RGBA")
    if a is not None:
        q.putalpha(a)
    return q


def build_tileset() -> None:
    # Prefer tiles that tile with near-zero seam (measured 2×2). Avoid autotile EDGE
    # cells and WallSimple frames — both stamp a cell outline when repeated.
    # Pure dirt only in the floor pool — (3,11) is green and caused a grass checkerboard.
    dirt = crop_tile(_FLOOR, 1, 8)
    mud = crop_tile(_FLOOR, 12, 15)  # darker twin of dirt center; tiles the same way
    tiles = [
        dirt,                                      # 0 dirt
        crop_tile(_FLOOR, 0, 11),                  # 1 dirt pebble
        crop_tile(_FLOOR, 1, 11),                  # 2 dirt pebble light
        crop_tile(_FLOOR, 0, 12),                  # 3 plain grass (verge only)
        mud,                                       # 4 mud (also used as path)
        crop_tile(_INT, 17, 13),                   # 5 cobble wall
        crop_tile(_INT, 16, 14),                   # 6 cobble wall variant
        tint(dirt, (80, 130, 210), 0.18),          # 7 alliance
        tint(dirt, (200, 70, 50), 0.18),           # 8 horde
        mud,                                       # 9 path
        crop_tile(_INT, 17, 14),                   # 10 flag
        crop_tile(_INT, 12, 13),                   # 11 pad
    ]
    for i, t in enumerate(tiles):
        trans = sum(1 for p in t.getdata() if p[3] < 8)
        if trans:
            raise SystemExit(f"tile {i} has {trans} transparent px — would checkerboard on GBA")
    strip = Image.new("RGBA", (TILE * len(tiles), TILE), (0, 0, 0, 0))
    for i, t in enumerate(tiles):
        strip.paste(t, (i * TILE, 0))
    strip = quantize_strip(strip)
    strip.save(OUT / "wsg_tiles.png")

    tsj = {
        "columns": len(tiles),
        "image": "wsg_tiles.png",
        "imagewidth": TILE * len(tiles),
        "imageheight": TILE,
        "margin": 0,
        "spacing": 0,
        "name": "wsg_tiles",
        "tilecount": len(tiles),
        "tiledversion": "1.11.0",
        "tilewidth": TILE,
        "tileheight": TILE,
        "type": "tileset",
        "version": "1.10",
    }
    (OUT / "wsg_tiles.tsj").write_text(json.dumps(tsj, indent=1))
    print(f"wsg_tiles.png  {len(tiles)} tiles (seamless NA fills)")


def build_map() -> None:
    first = 1
    g_floor = [
        first + T_FLOOR0, first + T_FLOOR0, first + T_FLOOR0,
        first + T_FLOOR1, first + T_FLOOR2,
    ]
    g_grass = first + T_GRASS
    g_wall = [first + T_WALL0, first + T_WALL1]
    g_a = first + T_BASE_A
    g_h = first + T_BASE_H
    g_path = first + T_PATH
    g_flag = first + T_FLAG

    def pick(variants: list[int], c: int, r: int) -> int:
        return variants[(c * 17 + r * 31) % len(variants)]

    ground = [0] * (W * H)
    solid = [0] * (W * H)

    for r in range(H):
        for c in range(W):
            ground[r * W + c] = pick(g_floor, c, r)

    def set_wall(c: int, r: int) -> None:
        if 0 <= c < W and 0 <= r < H:
            ground[r * W + c] = pick(g_wall, c, r)
            solid[r * W + c] = ground[r * W + c]

    # Perimeter
    for c in range(W):
        set_wall(c, 0)
        set_wall(c, H - 1)
    for r in range(H):
        set_wall(0, r)
        set_wall(W - 1, r)

    # Single grass ring just inside the wall (flat fill — reads as verge, not cells)
    for c in range(1, W - 1):
        ground[1 * W + c] = g_grass
        ground[(H - 2) * W + c] = g_grass
    for r in range(1, H - 1):
        ground[r * W + 1] = g_grass
        ground[r * W + (W - 2)] = g_grass

    # Base back walls + short side stubs (open toward mid)
    for r in range(6, 12):
        set_wall(1, r)
        set_wall(W - 2, r)
    for c in range(1, 4):
        set_wall(c, 5)
        set_wall(c, 12)
    for c in range(W - 4, W - 1):
        set_wall(c, 5)
        set_wall(c, 12)

    # Midfield choke pillars + side cover
    for r, cols in (
        (3, (8, 9, 13, 14, 15, 16, 20, 21)),
        (4, (8, 9, 13, 16, 20, 21)),
        (13, (8, 9, 13, 16, 20, 21)),
        (14, (8, 9, 13, 14, 15, 16, 20, 21)),
    ):
        for c in cols:
            set_wall(c, r)

    # Faction floors inside bases (matches arena.tish rects)
    for r in range(6, 12):
        for c in range(2, 5):
            if solid[r * W + c] == 0:
                ground[r * W + c] = g_a
        for c in range(W - 5, W - 2):
            if solid[r * W + c] == 0:
                ground[r * W + c] = g_h

    # East–west mud run
    for r in range(7, 11):
        for c in range(5, 25):
            if solid[r * W + c] == 0 and ground[r * W + c] not in (g_a, g_h):
                ground[r * W + c] = g_path

    # Flag stands
    if solid[9 * W + 3] == 0:
        ground[9 * W + 3] = g_flag
    if solid[9 * W + 26] == 0:
        ground[9 * W + 26] = g_flag

    def layer(name, data, lid):
        return {
            "type": "tilelayer", "name": name, "id": lid,
            "width": W, "height": H, "x": 0, "y": 0,
            "opacity": 1, "visible": True, "data": data,
        }

    m = {
        "type": "map", "orientation": "orthogonal", "renderorder": "right-down",
        "infinite": False, "width": W, "height": H,
        "tilewidth": TILE, "tileheight": TILE,
        "nextlayerid": 3, "nextobjectid": 1,
        "version": "1.10", "tiledversion": "1.11.0",
        "tilesets": [{"firstgid": first, "source": "wsg_tiles.tsj"}],
        "layers": [layer("Ground", ground, 1), layer("Solid", solid, 2)],
    }
    (OUT / "wsg.tmj").write_text(json.dumps(m, indent=1))
    print(f"wsg.tmj  {W}x{H} dirt valley CTF (open bases + mid run)")


# (folder under Actor/Character, idle frame col)
CLASSES = [
    ("Knight", 0),           # warrior
    ("KnightGold", 0),       # paladin
    ("Hunter", 0),           # hunter
    ("CamouflageGreen", 0),  # rogue
    ("Monk", 0),             # priest
    ("Caveman", 0),          # shaman
    ("Master", 0),           # mage
    ("DemonRed", 0),         # warlock
    ("CaveLion", 0),         # druid
]


def idle_frame(folder: str, col: int = 0) -> Image.Image:
    path = NA / "Actor" / "Character" / folder / "SpriteSheet.png"
    if not path.exists():
        # Some characters only have SeparateAnim/Down.png
        alt = NA / "Actor" / "Character" / folder / "SeparateAnim" / "Down.png"
        if alt.exists():
            src = Image.open(alt).convert("RGBA")
            return src.crop((0, 0, TILE, TILE))
        raise FileNotFoundError(path)
    src = Image.open(path).convert("RGBA")
    # Typical sheet: cols = walk cycle, rows = facing. Take down-idle.
    return src.crop((col * TILE, 0, col * TILE + TILE, TILE))


def team_badge(im: Image.Image, team: str) -> Image.Image:
    out = im.copy()
    d = ImageDraw.Draw(out)
    color = (70, 140, 255, 255) if team == "a" else (240, 80, 60, 255)
    # Corner pip so Alliance/Horde read at a glance
    d.rectangle([0, 0, 3, 3], fill=color)
    return out


def build_actors() -> None:
    frames: list[Image.Image] = []
    # 0..8 alliance classes, 9..17 horde classes
    for team in ("a", "h"):
        for folder, col in CLASSES:
            fr = idle_frame(folder, col)
            fr = team_badge(fr, team)
            if team == "h":
                fr = tint(fr, (200, 60, 40), 0.25)
            else:
                fr = tint(fr, (50, 90, 200), 0.18)
            frames.append(fr)

    # 18 flag A, 19 flag H, 20 reticle, 21 bolt
    flag_a = Image.new("RGBA", (TILE, TILE), (0, 0, 0, 0))
    d = ImageDraw.Draw(flag_a)
    d.rectangle([6, 1, 8, 14], fill=(30, 40, 80, 255))
    d.polygon([(8, 1), (14, 4), (8, 7)], fill=(80, 150, 255, 255))
    frames.append(flag_a)

    flag_h = Image.new("RGBA", (TILE, TILE), (0, 0, 0, 0))
    d = ImageDraw.Draw(flag_h)
    d.rectangle([6, 1, 8, 14], fill=(80, 20, 20, 255))
    d.polygon([(8, 1), (14, 4), (8, 7)], fill=(240, 90, 50, 255))
    frames.append(flag_h)

    ret = Image.new("RGBA", (TILE, TILE), (0, 0, 0, 0))
    d = ImageDraw.Draw(ret)
    d.rectangle([0, 0, 15, 15], outline=(255, 220, 60, 255))
    frames.append(ret)

    bolt = Image.new("RGBA", (TILE, TILE), (0, 0, 0, 0))
    d = ImageDraw.Draw(bolt)
    d.ellipse([4, 4, 11, 11], fill=(220, 230, 255, 255), outline=(40, 40, 80, 255))
    frames.append(bolt)

    # 22..29 floating health bar, fill steps 1/8 .. 8/8 (frame 22 = 1/8, frame 29 = full). The bar
    # is drawn in the TOP rows of the 16x16 cell so the sprite can sit directly over a unit's head.
    for step in range(1, 9):
        bar = Image.new("RGBA", (TILE, TILE), (0, 0, 0, 0))
        d = ImageDraw.Draw(bar)
        d.rectangle([1, 0, 14, 4], fill=(20, 20, 24, 255), outline=(0, 0, 0, 255))
        if step <= 2:
            col = (220, 60, 50, 255)
        elif step <= 4:
            col = (230, 180, 50, 255)
        else:
            col = (70, 210, 90, 255)
        d.rectangle([2, 1, 2 + (12 * step) // 8, 3], fill=col)
        frames.append(bar)

    # 30..39 red digits, 40..49 green digits — floating combat text is drawn as SPRITES, not with
    # text_draw: a text_draw repaint measured ~1,000 ticks (a quarter of a 4,389-tick frame), while
    # a sprite reposition is ~3. Digits are 5x7 in the top-left of the cell so two of them read as
    # one number over a unit's head.
    DIGITS = [
        (0, 0b111, 0b101, 0b101, 0b101, 0b111), (0, 0b010, 0b110, 0b010, 0b010, 0b111),
        (0, 0b111, 0b001, 0b111, 0b100, 0b111), (0, 0b111, 0b001, 0b111, 0b001, 0b111),
        (0, 0b101, 0b101, 0b111, 0b001, 0b001), (0, 0b111, 0b100, 0b111, 0b001, 0b111),
        (0, 0b111, 0b100, 0b111, 0b101, 0b111), (0, 0b111, 0b001, 0b010, 0b010, 0b010),
        (0, 0b111, 0b101, 0b111, 0b101, 0b111), (0, 0b111, 0b101, 0b111, 0b001, 0b111),
    ]
    for col in ((255, 90, 70, 255), (90, 230, 110, 255)):
        for rows in DIGITS:
            g = Image.new("RGBA", (TILE, TILE), (0, 0, 0, 0))
            d = ImageDraw.Draw(g)
            for ry, bits in enumerate(rows):
                for rx in range(3):
                    if bits & (1 << (2 - rx)):
                        # 2x2 pixel blocks + a 1px black skirt so digits read over any ground tile
                        d.rectangle([1 + rx * 2, 1 + ry * 2, 2 + rx * 2, 2 + ry * 2], fill=col)
            frames.append(g)

    strip = Image.new("RGBA", (TILE * len(frames), TILE), (0, 0, 0, 0))
    for i, fr in enumerate(frames):
        strip.paste(fr, (i * TILE, 0), fr)
    strip = quantize_strip(strip)
    strip.save(OUT / "wsg16.png")
    print(f"wsg16.png  {len(frames)} frames (9 classes × 2 teams + flag/FX/bars/digits)")


# ── skill kits (select screen) ─────────────────────────────────────────────
# Slot order matches match controls:
#   0 A / L+U · 1 L+D · 2 L+L · 3 L+R · 4 L+A · 5 L+B · 6 B · 7 L+Select
# Icons are relative paths under assets/ninja-adventure/Ui/Skill Icon/ (itch CC0).

def _kit(*pairs: tuple[str, str]) -> list[tuple[str, str]]:
    assert len(pairs) == 8
    return list(pairs)


# (class, spec) → 8× (short name ≤8 chars, icon path)
KITS: dict[tuple[int, int], list[tuple[str, str]]] = {
    # Warrior
    (0, 0): _kit(
        ("Mortal", "Items & Weapon/Kunai.png"),
        ("Charge", "Items & Weapon/Boot.png"),
        ("Overpowr", "Spell/AttackUpgrade.png"),
        ("Rend", "Spell/Cut.png"),
        ("Slam", "Job & Action/Punch.png"),
        ("Whirl", "Spell/Explosion.png"),
        ("Hamstr", "Spell/Downgrade.png"),
        ("Battle", "Spell/DefenseUpgrade.png"),
    ),
    (0, 1): _kit(
        ("Bloodth", "Spell/Cut.png"),
        ("Whirl", "Spell/Explosion.png"),
        ("Enrage", "Spell/AttackUpgrade.png"),
        ("Execute", "Items & Weapon/Kunai.png"),
        ("Rampage", "Job & Action/Punch.png"),
        ("Pierc", "Items & Weapon/Hook.png"),
        ("Berserk", "Spell/Fireball.png"),
        ("Intimd", "Spell/Death.png"),
    ),
    (0, 2): _kit(
        ("ShieldB", "Items & Weapon/Guard.png"),
        ("Revenge", "Spell/Counter.png"),
        ("Thunder", "Spell/BookThunder.png"),
        ("Disarm", "Spell/Downgrade.png"),
        ("LastSt", "Spell/DefenseUpgrade.png"),
        ("Concuss", "Job & Action/Punch.png"),
        ("ShieldW", "Items & Weapon/Armor.png"),
        ("Taunt", "Job & Action/Talk.png"),
    ),
    # Paladin
    (1, 0): _kit(
        ("HolyLt", "Spell/OrbLight.png"),
        ("Flash", "Spell/BookLight.png"),
        ("Consecr", "Spell/Explosion.png"),
        ("Purify", "Spell/Heal.png"),
        ("Beacon", "Spell/OrbLight.png"),
        ("Bless", "Job & Action/Potion.png"),
        ("Aura", "Spell/DefenseUpgrade.png"),
        ("LayHand", "Spell/Heal.png"),
    ),
    (1, 1): _kit(
        ("Avenger", "Items & Weapon/Kunai.png"),
        ("HolySh", "Items & Weapon/Guard.png"),
        ("Hammer", "Job & Action/Punch.png"),
        ("Righte", "Spell/BookLight.png"),
        ("Sacred", "Spell/OrbLight.png"),
        ("BlessW", "Spell/MagicWeapon.png"),
        ("Divine", "Spell/DefenseUpgrade.png"),
        ("HandOf", "Spell/Counter.png"),
    ),
    (1, 2): _kit(
        ("Judgmnt", "Spell/BookLight.png"),
        ("Crusadr", "Items & Weapon/Boot.png"),
        ("DivineS", "Spell/Cut.png"),
        ("Exorcis", "Spell/OrbLight.png"),
        ("Consecr", "Spell/Explosion.png"),
        ("Seal", "Spell/BookDeath.png"),
        ("Repenta", "Spell/Downgrade.png"),
        ("Zeal", "Spell/AttackUpgrade.png"),
    ),
    # Hunter
    (2, 0): _kit(
        ("KillCmd", "Items & Weapon/Arrow.png"),
        ("Bestial", "Spell/AttackUpgrade.png"),
        ("MendPet", "Spell/Heal.png"),
        ("Intimd", "Spell/Death.png"),
        ("Dash", "Items & Weapon/Boot.png"),
        ("CallPet", "Job & Action/Interact.png"),
        ("Aspect", "Spell/Camouflage.png"),
        ("Feed", "Job & Action/Dish.png"),
    ),
    (2, 1): _kit(
        ("Aimed", "Items & Weapon/Arrow.png"),
        ("Multi", "Items & Weapon/Shuriken.png"),
        ("Arcane", "Spell/OrbLight.png"),
        ("Steady", "Items & Weapon/Arrow.png"),
        ("Truesh", "Spell/Vision.png"),
        ("Silenc", "Spell/Mist.png"),
        ("Distrct", "Job & Action/Talk.png"),
        ("Flare", "Spell/Fireball.png"),
    ),
    (2, 2): _kit(
        ("Raptor", "Items & Weapon/Kunai.png"),
        ("Wyvern", "Spell/BookPlant.png"),
        ("Exposur", "Spell/Downgrade.png"),
        ("Mongose", "Spell/Cut.png"),
        ("Counter", "Spell/Counter.png"),
        ("Trap", "Items & Weapon/Hook.png"),
        ("Deterr", "Spell/RockSpike.png"),
        ("WingCl", "Items & Weapon/Boot.png"),
    ),
    # Rogue
    (3, 0): _kit(
        ("Mutilat", "Spell/Cut.png"),
        ("Rupture", "Spell/Death.png"),
        ("Envenom", "Spell/BookPlant.png"),
        ("Garrote", "Items & Weapon/Kunai.png"),
        ("Poison", "Job & Action/Potion.png"),
        ("Vendett", "Spell/AttackUpgrade.png"),
        ("CheapSh", "Job & Action/Punch.png"),
        ("Kidney", "Spell/Explosion.png"),
    ),
    (3, 1): _kit(
        ("Sinistr", "Items & Weapon/Kunai.png"),
        ("BladeF", "Spell/Cut.png"),
        ("Adrenal", "Spell/AttackUpgrade.png"),
        ("Eviscer", "Spell/Death.png"),
        ("Slice", "Items & Weapon/Shuriken.png"),
        ("Riposte", "Spell/Counter.png"),
        ("Gouge", "Job & Action/Punch.png"),
        ("BladeFl", "Spell/Explosion.png"),
    ),
    (3, 2): _kit(
        ("Hemor", "Spell/Cut.png"),
        ("Shadow", "Spell/Camouflage.png"),
        ("Premed", "Spell/Vision.png"),
        ("Ghostly", "Spell/OrbDarkness.png"),
        ("Prep", "Spell/Upgrade.png"),
        ("Vanish", "Spell/Mist.png"),
        ("Ambush", "Items & Weapon/Kunai.png"),
        ("Blind", "Spell/BookDarkness.png"),
    ),
    # Priest
    (4, 0): _kit(
        ("Penance", "Spell/OrbLight.png"),
        ("Shield", "Items & Weapon/Guard.png"),
        ("Smite", "Spell/BookLight.png"),
        ("Pain", "Spell/BookDarkness.png"),
        ("FlashH", "Spell/Heal.png"),
        ("PowerW", "Spell/DefenseUpgrade.png"),
        ("ManaBurn", "Spell/Alchemy.png"),
        ("Fear", "Spell/Death.png"),
    ),
    (4, 1): _kit(
        ("Heal", "Spell/Heal.png"),
        ("Renew", "Job & Action/Potion.png"),
        ("Prayer", "Spell/BookLight.png"),
        ("HolyNova", "Spell/Explosion.png"),
        ("Guardian", "Spell/OrbLight.png"),
        ("Spirit", "Spell/Upgrade.png"),
        ("Bind", "Spell/Mist.png"),
        ("Chastis", "Spell/BookLight.png"),
    ),
    (4, 2): _kit(
        ("MindFlay", "Spell/OrbDarkness.png"),
        ("ShadowW", "Spell/BookDarkness.png"),
        ("Vampir", "Spell/Necromancy.png"),
        ("Devour", "Spell/Death.png"),
        ("MindBlast", "Spell/Explosion.png"),
        ("Silence", "Spell/Mist.png"),
        ("Fade", "Spell/Camouflage.png"),
        ("Psychic", "Spell/BookDeath.png"),
    ),
    # Shaman
    (5, 0): _kit(
        ("Lightng", "Spell/BookThunder.png"),
        ("LavaB", "Spell/Fireball.png"),
        ("EarthSh", "Spell/RockSpike.png"),
        ("FlameSh", "Spell/OrbFire.png"),
        ("Hex", "Spell/Downgrade.png"),
        ("Thunder", "Spell/BookThunder.png"),
        ("GhostW", "Items & Weapon/Boot.png"),
        ("Element", "Spell/OrbWater.png"),
    ),
    (5, 1): _kit(
        ("Storms", "Spell/BookThunder.png"),
        ("LavaL", "Spell/OrbFire.png"),
        ("Windfry", "Items & Weapon/Kunai.png"),
        ("FrostB", "Spell/BookIce.png"),
        ("Feral", "Spell/AttackUpgrade.png"),
        ("Maelstr", "Spell/Explosion.png"),
        ("SpiritW", "Items & Weapon/Boot.png"),
        ("Purge", "Spell/Upgrade.png"),
    ),
    (5, 2): _kit(
        ("Wave", "Spell/OrbWater.png"),
        ("Riptide", "Spell/WaterCanon.png"),
        ("ChainH", "Spell/Heal.png"),
        ("EarthSh", "Items & Weapon/Guard.png"),
        ("Healing", "Job & Action/Potion.png"),
        ("Purify", "Spell/Upgrade.png"),
        ("SpiritL", "Spell/OrbLight.png"),
        ("Totem", "Job & Action/Plant.png"),
    ),
    # Mage
    (6, 0): _kit(
        ("ArcaneB", "Spell/OrbLight.png"),
        ("Missiles", "Spell/BookLight.png"),
        ("Barrage", "Spell/Explosion.png"),
        ("Explosion", "Spell/Explosion.png"),
        ("Power", "Spell/AttackUpgrade.png"),
        ("Slow", "Spell/Downgrade.png"),
        ("Blink", "Items & Weapon/Boot.png"),
        ("Counter", "Spell/Counter.png"),
    ),
    (6, 1): _kit(
        ("Fireball", "Spell/Fireball.png"),
        ("Pyroblst", "Spell/OrbFire.png"),
        ("Scorch", "Spell/BookFire.png"),
        ("Flamestr", "Spell/Explosion.png"),
        ("Combust", "Spell/Alchemy.png"),
        ("BlastWv", "Spell/Explosion.png"),
        ("FireBlst", "Items & Weapon/Boot.png"),
        ("Molten", "Spell/OrbFire.png"),
    ),
    (6, 2): _kit(
        ("Frostblt", "Spell/BookIce.png"),
        ("IceLanc", "Spell/OrbWater.png"),
        ("Cone", "Meteo/Snow.png"),
        ("Nova", "Spell/Explosion.png"),
        ("Freeze", "Spell/BookIce.png"),
        ("Shield", "Items & Weapon/Guard.png"),
        ("Blink", "Items & Weapon/Boot.png"),
        ("IceBlok", "Spell/DefenseUpgrade.png"),
    ),
    # Warlock
    (7, 0): _kit(
        ("Corrupt", "Spell/BookDarkness.png"),
        ("Agony", "Spell/Death.png"),
        ("Unstable", "Spell/OrbDarkness.png"),
        ("Drain", "Spell/Necromancy.png"),
        ("Haunt", "Spell/Mist.png"),
        ("Curse", "Spell/Downgrade.png"),
        ("Fear", "Spell/BookDeath.png"),
        ("Seed", "Spell/BookPlant.png"),
    ),
    (7, 1): _kit(
        ("ShadowB", "Spell/OrbDarkness.png"),
        ("HandOfG", "Spell/BookDarkness.png"),
        ("SoulFir", "Spell/Fireball.png"),
        ("Demonf", "Spell/AttackUpgrade.png"),
        ("Hellfir", "Spell/Explosion.png"),
        ("Summon", "Job & Action/Interact.png"),
        ("HealthF", "Spell/Heal.png"),
        ("Soulst", "Spell/Alchemy.png"),
    ),
    (7, 2): _kit(
        ("Incinert", "Spell/Fireball.png"),
        ("ChaosB", "Spell/OrbFire.png"),
        ("Conflag", "Spell/BookFire.png"),
        ("Immolat", "Spell/OrbFire.png"),
        ("RainFir", "Spell/Explosion.png"),
        ("Shadowb", "Spell/OrbDarkness.png"),
        ("Burning", "Items & Weapon/Boot.png"),
        ("Havoc", "Spell/Permutation.png"),
    ),
    # Druid
    (8, 0): _kit(
        ("Wrath", "Spell/OrbPlant.png"),
        ("Starfir", "Spell/OrbLight.png"),
        ("Moonfir", "Spell/BookPlant.png"),
        ("Insect", "Spell/Downgrade.png"),
        ("Typhoon", "Spell/BookWind.png"),
        ("Roots", "Job & Action/Plant.png"),
        ("Barkskn", "Items & Weapon/Armor.png"),
        ("Faerie", "Spell/Vision.png"),
    ),
    (8, 1): _kit(
        ("Shred", "Spell/Cut.png"),
        ("Rake", "Items & Weapon/Kunai.png"),
        ("Rip", "Spell/Death.png"),
        ("Feroc", "Spell/AttackUpgrade.png"),
        ("Swipe", "Spell/Explosion.png"),
        ("Dash", "Items & Weapon/Boot.png"),
        ("Prowl", "Spell/Camouflage.png"),
        ("Maim", "Job & Action/Punch.png"),
    ),
    (8, 2): _kit(
        ("Healing", "Spell/Heal.png"),
        ("Rejuv", "Job & Action/Potion.png"),
        ("Regrow", "Spell/OrbPlant.png"),
        ("Swiftmd", "Spell/BookPlant.png"),
        ("WildGr", "Spell/Heal.png"),
        ("Lifeblo", "Spell/Upgrade.png"),
        ("Barkskn", "Items & Weapon/Armor.png"),
        ("Nourish", "Job & Action/Harvest.png"),
    ),
}


def _load_skill_icon(rel: str) -> Image.Image:
    path = SKILL_ICON_DIR / rel
    if not path.exists():
        raise FileNotFoundError(path)
    src = Image.open(path).convert("RGBA")
    # Pack art is 24×24; GBA sheets in this example are 16×16.
    return src.resize((TILE, TILE), Image.Resampling.NEAREST)


def build_skills() -> dict[str, int]:
    """Pack used icons into skills16.png; return path→frame map."""
    order: list[str] = []
    seen: set[str] = set()
    for kit in KITS.values():
        for _name, rel in kit:
            if rel not in seen:
                seen.add(rel)
                order.append(rel)

    frames = [_load_skill_icon(rel) for rel in order]
    # Frame 0 blank for empty slots
    blank = Image.new("RGBA", (TILE, TILE), (0, 0, 0, 0))
    frames.insert(0, blank)
    index = {rel: i + 1 for i, rel in enumerate(order)}

    strip = Image.new("RGBA", (TILE * len(frames), TILE), (0, 0, 0, 0))
    for i, fr in enumerate(frames):
        strip.paste(fr, (i * TILE, 0), fr)
    strip = quantize_strip(strip)
    strip.save(OUT / "skills16.png")
    print(f"skills16.png  {len(frames)} frames ({len(order)} icons + blank)")
    return index


def write_skill_kit(icon_index: dict[str, int]) -> None:
    """Emit skill_kit.tish — icon frames + name/key helpers for the UI select screen."""
    ico = [0] * (9 * 3 * 12)
    names: dict[tuple[int, int, int], str] = {}
    for (cls, spec), kit in KITS.items():
        base = cls * 36 + spec * 12
        for slot, (name, rel) in enumerate(kit):
            ico[base + slot] = icon_index[rel]
            names[(cls, spec, slot)] = name

    keys = ["A", "D", "L", "R", "LA", "LB", "B", "LS"]

    lines: list[str] = [
        "// AUTO-GENERATED by scripts/gen_wsg.py — do not edit.",
        "// Skill names + icon frames for the Warsong select screen.",
        "// Icons: Ninja Adventure Skill Icon pack (Pixel-boy & AAA, CC0 / itch.io).",
        "",
        f"export let SKILL_ICO: i32[] = [{', '.join(str(v) for v in ico)}]",
        "",
        "export function classSkillIcon(cls: i32, spec: i32, slot: i32): i32 {",
        "  if (cls < 0 || cls >= 9) { return 0 }",
        "  if (spec < 0 || spec >= 3) { return 0 }",
        "  if (slot < 0 || slot >= 12) { return 0 }",
        "  return SKILL_ICO[cls * 36 + spec * 12 + slot]",
        "}",
        "",
        "export function skillKey(slot: i32) {",
    ]
    for i, k in enumerate(keys):
        lines.append(f'  if (slot === {i}) {{ return "{k}" }}')
    lines.append('  return ""')
    lines.append("}")
    lines.append("")
    lines.append("export function skillName(cls: i32, spec: i32, slot: i32) {")
    for cls in range(9):
        for spec in range(3):
            for slot in range(8):
                nm = names[(cls, spec, slot)]
                lines.append(
                    f'  if (cls === {cls} && spec === {spec} && slot === {slot}) {{ return "{nm}" }}'
                )
    lines.append('  return ""')
    lines.append("}")
    lines.append("")

    out = SRC / "skill_kit.tish"
    out.write_text("\n".join(lines) + "\n")
    print(f"skill_kit.tish  {len(names)} named skills")



def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    SRC.mkdir(parents=True, exist_ok=True)
    build_tileset()
    build_map()
    build_actors()
    icon_index = build_skills()
    write_skill_kit(icon_index)


if __name__ == "__main__":
    main()
