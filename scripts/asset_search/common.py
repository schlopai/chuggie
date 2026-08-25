"""Shared helpers for the asset search index: paths, tokenizer, image fingerprints.

Dependency-light: only Pillow + numpy (already used elsewhere in scripts/).
"""
from __future__ import annotations

import hashlib
import os
import re
from typing import List, Optional

import numpy as np
from PIL import Image

# ---------------------------------------------------------------------------
# Paths
# ---------------------------------------------------------------------------
# scripts/asset_search/common.py -> repo root is two dirs up from scripts/.
REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
ASSETS_ROOT = os.path.join(REPO_ROOT, "assets", "ninja-adventure")
CATALOG_DIR = os.path.join(ASSETS_ROOT, "catalog")
SEARCH_DIR = os.path.join(CATALOG_DIR, "search")

# Canonical fingerprint size. Native tiles are 16x16, so a same-size tile
# fingerprints to itself byte-for-byte -> exact matching "just works".
FP = 16


def resolve_asset(file: str) -> Optional[str]:
    """Resolve a catalog `file` field to an absolute path.

    Catalogs are inconsistent: tilesets/items use paths relative to the pack
    root (``Backgrounds/Tilesets/...``) while actors use repo-relative paths
    (``assets/ninja-adventure/Actor/...``). Try both.
    """
    if not file:
        return None
    cands = []
    if os.path.isabs(file):
        cands.append(file)
    else:
        # repo-relative (actors) and pack-relative (everything else)
        cands.append(os.path.join(REPO_ROOT, file))
        cands.append(os.path.join(ASSETS_ROOT, file))
    for c in cands:
        if os.path.isfile(c):
            return c
    return None


# ---------------------------------------------------------------------------
# Text tokenizer (shared by the BM25 index and queries)
# ---------------------------------------------------------------------------
_TOKEN_RE = re.compile(r"[a-z0-9]+")
_STOP = frozenset(
    "the a an of on and or with to in is are for that this it its as at by "
    "from into over under near not no left right top bottom center centre "
    "col cols row rows tile tiles px".split()
)


def tokenize(text: str) -> List[str]:
    if not text:
        return []
    return [t for t in _TOKEN_RE.findall(text.lower()) if len(t) >= 2 and t not in _STOP]


# ---------------------------------------------------------------------------
# Image fingerprints
# ---------------------------------------------------------------------------
def _canon(img: Image.Image) -> np.ndarray:
    """Return a canonical (FP, FP, 4) uint8 RGBA array.

    Fully-transparent pixels are flattened to (0,0,0,0) so that two visually
    identical tiles which merely differ in the RGB *behind* transparent pixels
    hash and compare as identical.
    """
    img = img.convert("RGBA")
    if img.size != (FP, FP):
        # BOX = area-average: a faithful downscale for representative colour.
        img = img.resize((FP, FP), Image.BOX)
    a = np.asarray(img, dtype=np.uint8).copy()
    if a.shape != (FP, FP, 4):  # paranoia for odd modes
        a = np.asarray(img.convert("RGBA"), dtype=np.uint8).copy()
    transparent = a[:, :, 3] == 0
    a[transparent] = 0
    return a


def _dhash(thumb: np.ndarray) -> np.uint64:
    """64-bit difference hash over greyscale of the canonical thumb."""
    # luminance, alpha-weighted so transparent pixels read as black
    rgb = thumb[:, :, :3].astype(np.float32)
    alpha = (thumb[:, :, 3:4].astype(np.float32)) / 255.0
    gray = (0.299 * rgb[:, :, 0] + 0.587 * rgb[:, :, 1] + 0.114 * rgb[:, :, 2]) * alpha[:, :, 0]
    # 8x8 comparison grid -> compare each pixel to its right neighbour
    small = np.asarray(
        Image.fromarray(gray.astype(np.uint8)).resize((9, 8), Image.BOX), dtype=np.int16
    )
    diff = small[:, 1:] > small[:, :-1]  # (8,8) bool
    bits = diff.flatten()
    val = np.uint64(0)
    for b in bits:
        val = np.uint64(val << np.uint64(1)) | np.uint64(1 if b else 0)
    return val


class Fingerprint:
    __slots__ = ("thumb", "phash", "avg", "exact")

    def __init__(self, thumb: np.ndarray):
        self.thumb = thumb  # (FP,FP,4) uint8
        self.phash = _dhash(thumb)  # uint64
        a = thumb.reshape(-1, 4).astype(np.float32)
        self.avg = a.mean(axis=0) / 255.0  # (4,) float in [0,1]
        # exact content hash of the canonical bytes
        self.exact = hashlib.blake2b(thumb.tobytes(), digest_size=16).hexdigest()

    @classmethod
    def from_image(cls, img: Image.Image) -> "Fingerprint":
        return cls(_canon(img))

    @classmethod
    def from_path(cls, path: str) -> "Fingerprint":
        with Image.open(path) as im:
            return cls(_canon(im))


def hamming64(a: np.ndarray, b: np.uint64) -> np.ndarray:
    """Vectorised popcount Hamming distance between a uint64 array and a scalar."""
    x = np.bitwise_xor(a.astype(np.uint64), np.uint64(b))
    # SWAR popcount for uint64
    x = x - ((x >> np.uint64(1)) & np.uint64(0x5555555555555555))
    x = (x & np.uint64(0x3333333333333333)) + ((x >> np.uint64(2)) & np.uint64(0x3333333333333333))
    x = (x + (x >> np.uint64(4))) & np.uint64(0x0F0F0F0F0F0F0F0F)
    return ((x * np.uint64(0x0101010101010101)) >> np.uint64(56)).astype(np.int32)
