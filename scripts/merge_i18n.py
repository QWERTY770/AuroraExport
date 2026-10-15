#!/usr/bin/env python3
"""Merge the i18n resources of genshin-wish-export and star-rail-warp-export.

Both upstream tools keep one flat "a.b.c" -> text JSON per language.  They share
most keys, some keys only exist in one of them and a few keys carry a
game-specific wording (window title, excel prefix, weapon wording, ...).

The merged file groups keys per language into:
  shared    - identical (or only present) in both games
  genshin   - game specific wording for Genshin Impact
  starrail  - game specific wording for Honkai: Star Rail

Lookup order in the application is <game> -> shared -> en-us -> zh-cn.
"""
from __future__ import annotations

import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SRC = ROOT / "assets" / "i18n"
OUT = SRC / "merged.json"

# locale code -> (genshin file stem, star rail file stem), both share file names
LOCALES = [
    ("zh-cn", "简体中文"),
    ("zh-tw", "繁體中文"),
    ("de-de", "Deutsch"),
    ("en-us", "English"),
    ("es-es", "Español"),
    ("fr-fr", "Français"),
    ("id-id", "Indonesia"),
    ("ja-jp", "日本語"),
    ("ko-kr", "한국어"),
    ("pt-pt", "Português"),
    ("ru-ru", "Pусский"),
    ("th-th", "ภาษาไทย"),
    ("vi-vn", "Tiếng Việt"),
]


def load(game: str, stem: str) -> dict[str, str]:
    path = SRC / game / f"{stem}.json"
    with path.open(encoding="utf-8") as fh:
        return json.load(fh)


def layered(game: str, stem: str) -> dict[str, str]:
    """Reproduce upstream i18n.js layering for one language."""
    base_zh = load(game, "简体中文")
    if stem == "简体中文":
        return dict(base_zh)
    if stem == "繁體中文":
        merged = dict(base_zh)
        merged.update(load(game, stem))
        return merged
    merged = dict(base_zh)
    merged.update(load(game, "English"))
    merged.update(load(game, stem))
    return merged


def main() -> int:
    merged: dict[str, dict[str, dict[str, str]]] = {}
    stats = []
    for code, stem in LOCALES:
        g = layered("genshin", stem)
        s = layered("starrail", stem)
        shared: dict[str, str] = {}
        genshin: dict[str, str] = {}
        starrail: dict[str, str] = {}
        for key in sorted(set(g) | set(s)):
            gv, sv = g.get(key), s.get(key)
            if gv is not None and sv is not None:
                if gv == sv:
                    shared[key] = gv
                else:
                    genshin[key] = gv
                    starrail[key] = sv
            elif gv is not None:
                shared[key] = gv
            else:
                shared[key] = sv
        merged[code] = {"shared": shared, "genshin": genshin, "starrail": starrail}
        stats.append((code, len(shared), len(genshin), len(starrail)))

    OUT.write_text(
        json.dumps(merged, ensure_ascii=False, indent=1), encoding="utf-8"
    )
    for code, a, b, c in stats:
        print(f"{code:6s} shared={a:3d} genshin-only={b:3d} starrail-only={c:3d}")
    print(f"written {OUT} ({OUT.stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
