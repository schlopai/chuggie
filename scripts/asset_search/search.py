"""Query the asset search index: text (BM25), image (fingerprint), tilemap.

Loads the artifacts produced by build.py and answers queries in-memory.
No server framework, no ML deps — Pillow + numpy only.
"""
from __future__ import annotations

import json
import math
import os
from collections import Counter, defaultdict
from typing import Dict, List, Optional, Union

import numpy as np
from PIL import Image

from asset_search.common import (
    FP,
    SEARCH_DIR,
    Fingerprint,
    hamming64,
    resolve_asset,
    tokenize,
)

_FLAT = FP * FP * 4
_MAX_DIST = math.sqrt(_FLAT)  # worst-case L2 over normalised RGBA


def _score_from_dist(dist: np.ndarray) -> np.ndarray:
    return np.clip(1.0 - dist / _MAX_DIST, 0.0, 1.0)


class AssetSearch:
    def __init__(self, search_dir: str = SEARCH_DIR):
        self.dir = search_dir
        self._load_text()
        self._load_images()

    # ------------------------------------------------------------------ text
    def _load_text(self):
        self.records: List[dict] = []
        path = os.path.join(self.dir, "records.jsonl")
        with open(path) as f:
            for line in f:
                line = line.strip()
                if line:
                    self.records.append(json.loads(line))
        # BM25 index
        self._doc_tokens = [Counter(tokenize(r.get("text", ""))) for r in self.records]
        self._doc_len = np.array([sum(c.values()) for c in self._doc_tokens], dtype=np.float32)
        self._avgdl = float(self._doc_len.mean()) if len(self._doc_len) else 0.0
        df: Dict[str, int] = defaultdict(int)
        for c in self._doc_tokens:
            for term in c:
                df[term] += 1
        N = max(1, len(self.records))
        self._idf = {t: math.log(1 + (N - n + 0.5) / (n + 0.5)) for t, n in df.items()}
        # postings: term -> list of (doc_idx, tf)
        self._postings: Dict[str, List] = defaultdict(list)
        for i, c in enumerate(self._doc_tokens):
            for term, tf in c.items():
                self._postings[term].append((i, tf))

    def search_text(self, query: str, kind: Optional[Union[str, List[str]]] = None,
                    limit: int = 10) -> List[dict]:
        k1, b = 1.5, 0.75
        q_terms = set(tokenize(query))
        scores = np.zeros(len(self.records), dtype=np.float32)
        for term in q_terms:
            idf = self._idf.get(term)
            if idf is None:
                continue
            for i, tf in self._postings[term]:
                denom = tf + k1 * (1 - b + b * self._doc_len[i] / (self._avgdl or 1.0))
                scores[i] += idf * (tf * (k1 + 1)) / denom
        kinds = {kind} if isinstance(kind, str) else (set(kind) if kind else None)
        order = np.argsort(-scores)
        out = []
        for i in order:
            if scores[i] <= 0:
                break
            r = self.records[i]
            if kinds and r["kind"] not in kinds:
                continue
            out.append({**{k: r[k] for k in ("id", "kind", "file", "name") if k in r},
                        "tags": r.get("tags", []),
                        "score": round(float(scores[i]), 4),
                        **{k: r[k] for k in ("col", "row", "gid", "area", "grid", "tile_size") if k in r}})
            if len(out) >= limit:
                break
        return out

    # ----------------------------------------------------------------- image
    def _load_images(self):
        npz = np.load(os.path.join(self.dir, "fingerprints.npz"))
        self._thumb = npz["thumb"]              # (M,FP,FP,4) uint8
        self._phash = npz["phash"]              # (M,) uint64
        self._db = self._thumb.reshape(len(self._thumb), -1).astype(np.float32) / 255.0
        self._db_sq = (self._db ** 2).sum(axis=1)
        self.fp_meta: List[dict] = []
        with open(os.path.join(self.dir, "fp_meta.jsonl")) as f:
            for line in f:
                line = line.strip()
                if line:
                    self.fp_meta.append(json.loads(line))
        self._exact: Dict[str, List[int]] = defaultdict(list)
        for i, m in enumerate(self.fp_meta):
            if m.get("exact"):
                self._exact[m["exact"]].append(i)
        self._kind_idx: Dict[str, np.ndarray] = {}
        for k in ("tile", "image"):
            self._kind_idx[k] = np.array([i for i, m in enumerate(self.fp_meta) if m["kind"] == k],
                                         dtype=np.int64)

    def _subset(self, kind: Optional[str], tileset: Optional[str]) -> np.ndarray:
        if kind in ("tile", "image"):
            idx = self._kind_idx[kind]
        else:
            idx = np.arange(len(self.fp_meta), dtype=np.int64)
        if tileset:
            idx = np.array([i for i in idx if self.fp_meta[i].get("file") == tileset], dtype=np.int64)
        return idx

    def _rank_one(self, fp: Fingerprint, idx: np.ndarray, limit: int) -> List[dict]:
        # exact first
        hits = [i for i in self._exact.get(fp.exact, []) if i in set(idx.tolist())] if len(idx) < len(self.fp_meta) else self._exact.get(fp.exact, [])
        results, seen = [], set()
        for i in hits:
            results.append(self._result(i, 1.0, exact=True))
            seen.add(i)
            if len(results) >= limit:
                return results
        q = fp.thumb.reshape(-1).astype(np.float32) / 255.0
        sub = self._db[idx]
        dist = np.sqrt(np.maximum(self._db_sq[idx] + (q ** 2).sum() - 2.0 * sub.dot(q), 0.0))
        order = np.argsort(dist)
        for j in order:
            i = int(idx[j])
            if i in seen:
                continue
            results.append(self._result(i, float(_score_from_dist(dist[j])), exact=dist[j] == 0))
            if len(results) >= limit:
                break
        return results

    def _result(self, i: int, score: float, exact: bool = False) -> dict:
        m = self.fp_meta[i]
        out = {k: m[k] for k in ("kind", "file", "name", "col", "row", "gid", "group") if k in m}
        out["score"] = round(score, 4)
        out["exact"] = bool(exact)
        return out

    def search_image(self, image: Union[str, Image.Image], limit: int = 10,
                     kind: Optional[str] = None, tileset: Optional[str] = None) -> List[dict]:
        fp = _to_fp(image)
        idx = self._subset(kind, tileset)
        return self._rank_one(fp, idx, limit)

    # --------------------------------------------------------------- tilemap
    def match_tilemap(self, image: Union[str, Image.Image], tile_size: int = 16,
                      tileset: Optional[str] = None, kind: str = "tile") -> dict:
        img = _open(image)
        W, H = img.size
        cols, rows = W // tile_size, H // tile_size
        if cols == 0 or rows == 0:
            raise ValueError(f"image {W}x{H} smaller than tile_size {tile_size}")
        idx = self._subset(kind, tileset)
        idx_set = set(int(i) for i in idx)
        full = len(idx) == len(self.fp_meta)
        sub = self._db[idx]
        sub_sq = self._db_sq[idx]

        # fingerprint every cell
        cell_fps = []
        for r in range(rows):
            for c in range(cols):
                box = (c * tile_size, r * tile_size, (c + 1) * tile_size, (r + 1) * tile_size)
                cell_fps.append(Fingerprint.from_image(img.crop(box)))
        Q = np.stack([f.thumb.reshape(-1).astype(np.float32) / 255.0 for f in cell_fps])  # (N,D)
        Q_sq = (Q ** 2).sum(axis=1)
        # batched distance^2: (N,M) = Nsq[:,None] + Msq[None,:] - 2 Q@sub.T
        d2 = Q_sq[:, None] + sub_sq[None, :] - 2.0 * Q.dot(sub.T)
        best = np.argmin(d2, axis=1)
        best_dist = np.sqrt(np.maximum(d2[np.arange(len(best)), best], 0.0))

        cells, grid, hist = [], [], Counter()
        n = 0
        for r in range(rows):
            grow = []
            for c in range(cols):
                fp = cell_fps[n]
                if not (fp.thumb[:, :, 3] > 0).any():
                    # fully transparent -> an empty map cell (gid 0), not a blank tile
                    cell = {"col": c, "row": r, "file": None, "gid": 0,
                            "tcol": None, "trow": None, "score": 1.0, "exact": True, "empty": True}
                    cells.append(cell)
                    grow.append({"gid": 0, "file": None, "score": 1.0})
                    n += 1
                    continue
                # exact override (robust to float noise on identical bytes)
                exact_hits = self._exact.get(fp.exact, [])
                if not full:
                    exact_hits = [i for i in exact_hits if int(i) in idx_set]
                if exact_hits:
                    gi = exact_hits[0]
                    score, exact = 1.0, True
                else:
                    gi = int(idx[best[n]])
                    score, exact = float(_score_from_dist(best_dist[n])), bool(best_dist[n] == 0)
                m = self.fp_meta[gi]
                cell = {"col": c, "row": r, "file": m.get("file"), "gid": m.get("gid"),
                        "tcol": m.get("col"), "trow": m.get("row"),
                        "score": round(score, 4), "exact": exact}
                cells.append(cell)
                grow.append({"gid": m.get("gid"), "file": m.get("file"), "score": round(score, 4)})
                hist[m.get("file")] += 1
                n += 1
            grid.append(grow)

        dominant = hist.most_common(1)[0][0] if hist else None
        return {
            "tile_size": tile_size,
            "width": cols,
            "height": rows,
            "dominant_tileset": dominant,
            "tileset_histogram": dict(hist),
            "mean_score": round(float(np.mean([c["score"] for c in cells])) if cells else 0.0, 4),
            "exact_fraction": round(float(np.mean([c["exact"] for c in cells])) if cells else 0.0, 4),
            "cells": cells,
            "grid": grid,
        }

    # ------------------------------------------------------------------ misc
    def get(self, record_id: str) -> Optional[dict]:
        for r in self.records:
            if r.get("id") == record_id:
                return r
        return None


def _open(image: Union[str, Image.Image]) -> Image.Image:
    if isinstance(image, Image.Image):
        return image.convert("RGBA")
    path = image if os.path.isfile(image) else (resolve_asset(image) or image)
    return Image.open(path).convert("RGBA")


def _to_fp(image: Union[str, Image.Image]) -> Fingerprint:
    return Fingerprint.from_image(_open(image))
