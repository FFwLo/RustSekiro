"""Convert the fonts embedded in Sekiro's Scaleform font files to TrueType.

    python tools/gfx_font.py            # every extracted/font/*/font.gfx -> extracted/font/<FontName>.ttf

The game ships its UI fonts as DefineFont3 tags (Flash glyph outlines, quadratic curves,
EM square 1024 at 20 units per point) inside font/<lang>_<use>/font.gfx. Unpack them first:
    sekiro-extract unpack <Sekiro dir> extracted font/engus_
"""
import struct
import sys
from pathlib import Path

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen

ROOT = Path(__file__).resolve().parent.parent / "extracted" / "font"
SCALE = 10  # SWF font units (20480 per EM) -> 2048 per EM


class Bits:
    def __init__(self, b, p):
        self.b, self.p, self.bit = b, p, 0

    def ub(self, n):
        v = 0
        for _ in range(n):
            v = (v << 1) | ((self.b[self.p] >> (7 - self.bit)) & 1)
            self.bit += 1
            if self.bit == 8:
                self.bit, self.p = 0, self.p + 1
        return v

    def sb(self, n):
        v = self.ub(n)
        return v - (1 << n) if n and v >> (n - 1) else v


def shape(b, p, pen):
    """Draw one SHAPE record into a pen (y flipped to point up)."""
    r = Bits(b, p)
    fill_bits, line_bits = r.ub(4), r.ub(4)
    x = y = 0
    open_ = False
    while True:
        if r.ub(1) == 0:  # style change or end
            flags = r.ub(5)
            if flags == 0:
                break
            if flags & 1:  # move to (absolute)
                n = r.ub(5)
                x, y = r.sb(n), r.sb(n)
                if open_:
                    pen.closePath()
                pen.moveTo((x / SCALE, -y / SCALE))
                open_ = True
            if flags & 2:
                r.ub(fill_bits)
            if flags & 4:
                r.ub(fill_bits)
            if flags & 8:
                r.ub(line_bits)
        elif r.ub(1):  # straight edge
            n = r.ub(4) + 2
            if r.ub(1):
                x, y = x + r.sb(n), y + r.sb(n)
            elif r.ub(1):
                y += r.sb(n)
            else:
                x += r.sb(n)
            pen.lineTo((x / SCALE, -y / SCALE))
        else:  # curved edge
            n = r.ub(4) + 2
            cx, cy = x + r.sb(n), y + r.sb(n)
            x, y = cx + r.sb(n), cy + r.sb(n)
            pen.qCurveTo((cx / SCALE, -cy / SCALE), (x / SCALE, -y / SCALE))
    if open_:
        pen.closePath()


def define_font3(b, p):
    flags = b[p + 2]
    wide_offsets, wide_codes, has_layout = flags & 0x08, flags & 0x04, flags & 0x80
    name_len = b[p + 4]
    name = b[p + 5:p + 5 + name_len].rstrip(b"\0").decode("latin1")
    q = p + 5 + name_len
    count = struct.unpack_from("<H", b, q)[0]
    q += 2
    fmt, size = ("<I", 4) if wide_offsets else ("<H", 2)
    table = q
    offsets = [struct.unpack_from(fmt, b, table + i * size)[0] for i in range(count + 1)]
    code_at = table + offsets[count]
    cfmt, csize = ("<H", 2) if wide_codes else ("<B", 1)
    codes = [struct.unpack_from(cfmt, b, code_at + i * csize)[0] for i in range(count)]
    q = code_at + count * csize
    ascent = descent = 0
    advances = [1024 * 20] * count
    if has_layout:
        ascent, descent, _leading = struct.unpack_from("<hhh", b, q)
        q += 6
        advances = list(struct.unpack_from(f"<{count}h", b, q))
    glyphs = []
    for i in range(count):
        pen = TTGlyphPen(None)
        shape(b, table + offsets[i], pen)
        glyphs.append(pen.glyph())
    return name, codes, glyphs, advances, ascent, descent


def convert(gfx: Path):
    b = gfx.read_bytes()
    if b[:3] not in (b"GFX", b"FWS"):
        raise ValueError(f"{gfx}: compressed or unknown header {b[:3]!r}")
    p = 8 + (5 + 4 * (b[8] >> 3) + 7) // 8 + 4
    while p < len(b):
        h = struct.unpack_from("<H", b, p)[0]
        tag, length = h >> 6, h & 63
        p += 2
        if length == 63:
            length = struct.unpack_from("<I", b, p)[0]
            p += 4
        if tag == 75:
            return build(*define_font3(b, p))
        if tag == 0:
            break
        p += length
    raise ValueError(f"{gfx}: no DefineFont3")


def build(name, codes, glyphs, advances, ascent, descent):
    order = [".notdef"] + [f"g{i}" for i in range(len(glyphs))]
    empty = TTGlyphPen(None).glyph()
    fb = FontBuilder(2048, isTTF=True)
    fb.setupGlyphOrder(order)
    fb.setupCharacterMap({c: f"g{i}" for i, c in enumerate(codes) if c})
    fb.setupGlyf({".notdef": empty, **{f"g{i}": g for i, g in enumerate(glyphs)}})
    glyf = fb.font["glyf"]
    metrics = {".notdef": (1024, 0)}
    for i in range(len(glyphs)):
        g = glyf[f"g{i}"]
        metrics[f"g{i}"] = (max(0, round(advances[i] / SCALE)), getattr(g, "xMin", 0))
    fb.setupHorizontalMetrics(metrics)
    asc, desc = round(ascent / SCALE) or 1800, round(descent / SCALE) or 450
    fb.setupHorizontalHeader(ascent=asc, descent=-desc)
    family = name.replace("FOT-", "Sekiro ")
    fb.setupNameTable({"familyName": family, "styleName": "Regular"})
    fb.setupOS2(sTypoAscender=asc, sTypoDescender=-desc, usWinAscent=asc, usWinDescent=desc)
    fb.setupPost()
    return name, fb.font


def main():
    for gfx in sorted(ROOT.glob("*/font.gfx")):
        name, font = convert(gfx)
        out = ROOT / f"{gfx.parent.name}.ttf"
        font.save(out)
        print(f"{gfx.parent.name}: {name} -> {out.name} ({len(font.getGlyphOrder()) - 1} glyphs)")


if __name__ == "__main__":
    sys.exit(main())
