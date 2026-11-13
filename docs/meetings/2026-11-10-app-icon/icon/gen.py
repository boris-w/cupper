#!/usr/bin/env python3
"""The cupper app icon: a small character peeking out of a cup, in six palettes.

Writes cupper-<palette>.svg next to this file. Each is a 1024 x 1024 master on
the macOS icon grid: an 824 x 824 tile with continuous corners, centred, with
the template's drop shadow. `latte` is the default icon.

Run: python3 gen.py
PNG: render each SVG at 1024 x 1024 on a transparent background, for example
  chrome --headless --default-background-color=00000000 --window-size=1024,1024 \
    --screenshot=cupper-latte.png cupper-latte.svg
"""
import math
import pathlib

OUT = pathlib.Path(__file__).resolve().parent

# bg: tile top/bottom; cup: top/bottom; inside: the print on the cup; pal: the
# character, light/dark; eye, cheek; shade: shadow colour; dark: dark tile.
PALETTES = {
    "latte": dict(bg=("#FAF3E8", "#EBDDC8"), cup=("#FFFFFF", "#F0E7DA"), inside="#D8C6AE",
                  pal=("#EDC08C", "#C28C54"), eye="#4A2C1C", cheek="#F2957A", shade="#5A3A1C"),
    "mocha": dict(bg=("#F8BC8E", "#E07E50"), cup=("#6E4533", "#41251B"), inside="#2E1810",
                  pal=("#FFF8EC", "#F2DCC2"), eye="#4A2C1C", cheek="#F79A72", shade="#7A2E0E"),
    "sky": dict(bg=("#E4EFFB", "#BCD5F1"), cup=("#FFFFFF", "#E9EFF7"), inside="#C3D2E6",
                pal=("#6C90D6", "#3A569A"), eye="#FFFFFF", cheek="#FFA6B8", shade="#1E3560"),
    "matcha": dict(bg=("#F1F5E6", "#DAE6C6"), cup=("#FFFFFF", "#EEF1E4"), inside="#CBD6B6",
                   pal=("#9CC074", "#5F8A42"), eye="#FFFBEE", cheek="#F6B08A", shade="#2F4A1C"),
    "ink": dict(bg=("#F8F8F6", "#E5E5E1"), cup=("#FFFFFF", "#ECECE8"), inside="#CFCFCB",
                pal=("#5A5C66", "#1D1E22"), eye="#FFFFFF", cheek="#FF9E8A", shade="#15161C"),
    "night": dict(bg=("#30323A", "#131417"), cup=("#FFF8EE", "#E6D9C8"), inside="#B9A892",
                  pal=("#FFB884", "#EE6A3A"), eye="#2A1710", cheek="#FFE0C2", shade="#000000", dark=True),
}

TILE = 824          # tile size on the 1024 canvas
RADIUS = 185.4      # corner radius of the macOS icon template at that size
SMOOTHING = 0.6     # share of the corner given to the easing curves


def continuous_rect(size, radius, smoothing):
    """A square with continuous-curvature corners: each corner is a circular arc
    with a cubic easing curve on either side, instead of an arc meeting a line."""
    p = (1 + smoothing) * radius
    arc_deg = 90 * (1 - smoothing)
    arc = math.sin(math.radians(arc_deg / 2)) * radius * math.sqrt(2)
    alpha = (90 - arc_deg) / 2
    p3_p4 = radius * math.tan(math.radians(alpha / 2))
    beta = 45 * smoothing
    c = p3_p4 * math.cos(math.radians(beta))
    d = c * math.tan(math.radians(beta))
    b = (p - arc - c - d) / 3
    a = 2 * b
    n = lambda v: f"{v:.2f}"
    r = n(radius)
    return (
        f"M{n(size - p)} 0 "
        f"c{n(a)} 0 {n(a + b)} 0 {n(a + b + c)} {n(d)} a{r} {r} 0 0 1 {n(arc)} {n(arc)} "
        f"c{n(d)} {n(c)} {n(d)} {n(b + c)} {n(d)} {n(a + b + c)} "
        f"L{n(size)} {n(size - p)} "
        f"c0 {n(a)} 0 {n(a + b)} {n(-d)} {n(a + b + c)} a{r} {r} 0 0 1 {n(-arc)} {n(arc)} "
        f"c{n(-c)} {n(d)} {n(-(b + c))} {n(d)} {n(-(a + b + c))} {n(d)} "
        f"L{n(p)} {n(size)} "
        f"c{n(-a)} 0 {n(-(a + b))} 0 {n(-(a + b + c))} {n(-d)} a{r} {r} 0 0 1 {n(-arc)} {n(-arc)} "
        f"c{n(-d)} {n(-c)} {n(-d)} {n(-(b + c))} {n(-d)} {n(-(a + b + c))} "
        f"L0 {n(p)} "
        f"c0 {n(-a)} 0 {n(-(a + b))} {n(d)} {n(-(a + b + c))} a{r} {r} 0 0 1 {n(arc)} {n(-arc)} "
        f"c{n(c)} {n(-d)} {n(b + c)} {n(-d)} {n(a + b + c)} {n(-d)} Z"
    )


# The artwork is drawn in a 1024 box and scaled down into the tile.
SCALE = TILE / 1024
SHAPE = continuous_rect(1024, RADIUS / SCALE, SMOOTHING)

CUP_BODY = "M266 476 H654 Q680 476 680 502 V596 A180 180 0 0 1 500 776 H420 A180 180 0 0 1 240 596 V502 Q240 476 266 476 Z"
CUP_HANDLE = "M664 530 H700 A72 72 0 0 1 700 674 H636"
HEAD = (460, 436, 180)


def is_light(colour):
    r, g, b = (int(colour[i:i + 2], 16) for i in (1, 3, 5))
    return .299 * r + .587 * g + .114 * b > 150


def tile(p):
    edge = "rgba(255,255,255,.07)" if p.get("dark") else "rgba(60,40,20,.10)"
    shine = ".26" if p.get("dark") else ".85"
    return f"""
    <path d="{SHAPE}" fill="url(#bg)"/>
    <g clip-path="url(#shape)">
      <path d="{SHAPE}" fill="none" stroke="{edge}" stroke-width="8"/>
      <path d="{SHAPE}" fill="none" stroke="url(#shine)" stroke-width="10"/>
    </g>"""


def defs(p):
    cx, cy, r = HEAD
    strength = 1.5 if p.get("dark") else 1.0
    shine = ".26" if p.get("dark") else ".85"
    return f"""
  <defs>
    <linearGradient id="bg" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="0" y2="1024">
      <stop offset="0" stop-color="{p["bg"][0]}"/><stop offset="1" stop-color="{p["bg"][1]}"/>
    </linearGradient>
    <linearGradient id="shine" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="0" y2="1024">
      <stop offset="0" stop-color="#fff" stop-opacity="{shine}"/>
      <stop offset=".35" stop-color="#fff" stop-opacity="0"/>
    </linearGradient>
    <clipPath id="shape"><path d="{SHAPE}"/></clipPath>
    <linearGradient id="cup" gradientUnits="userSpaceOnUse" x1="0" y1="476" x2="0" y2="776">
      <stop offset="0" stop-color="{p["cup"][0]}"/><stop offset="1" stop-color="{p["cup"][1]}"/>
    </linearGradient>
    <radialGradient id="pal" gradientUnits="userSpaceOnUse" cx="{cx - r * .35:.0f}" cy="{cy - r * .5:.0f}" r="{r * 1.7:.0f}">
      <stop offset="0" stop-color="{p["pal"][0]}"/><stop offset="1" stop-color="{p["pal"][1]}"/>
    </radialGradient>
    <filter id="tiledrop" x="-10%" y="-10%" width="120%" height="125%">
      <feDropShadow dx="0" dy="{10 / SCALE:.1f}" stdDeviation="{5 / SCALE:.1f}" flood-color="#000" flood-opacity=".3"/>
    </filter>
    <filter id="drop" x="-40%" y="-40%" width="180%" height="190%">
      <feDropShadow dx="0" dy="22" stdDeviation="24" flood-color="{p["shade"]}" flood-opacity="{.20 * strength:.2f}"/>
      <feDropShadow dx="0" dy="4" stdDeviation="5" flood-color="{p["shade"]}" flood-opacity="{.14 * strength:.2f}"/>
    </filter>
    <filter id="small" x="-60%" y="-60%" width="220%" height="240%">
      <feDropShadow dx="0" dy="6" stdDeviation="6" flood-color="{p["shade"]}" flood-opacity="{.22 * strength:.2f}"/>
    </filter>
    <filter id="soft" x="-60%" y="-200%" width="220%" height="500%"><feGaussianBlur stdDeviation="12"/></filter>
  </defs>"""


def eyes(p, cx, y, gap=62, r=36):
    """Round eyes looking up and to the right: a pupil in light eyes, a glint in dark ones."""
    out = ""
    for x in (cx - gap, cx + gap):
        out += f'<circle cx="{x}" cy="{y}" r="{r}" fill="{p["eye"]}"/>'
        if is_light(p["eye"]):
            out += f'<circle cx="{x + 9}" cy="{y - 7}" r="{r * .5:.1f}" fill="{p["pal"][1]}"/>'
        else:
            out += f'<circle cx="{x + 11}" cy="{y - 11}" r="{r * .3:.1f}" fill="#fff"/>'
    return out


def cheeks(p, cx, y, gap=118):
    return "".join(
        f'<ellipse cx="{x}" cy="{y}" rx="26" ry="17" fill="{p["cheek"]}" opacity=".75"/>'
        for x in (cx - gap, cx + gap)
    )


def paws(xs, y=484):
    return "".join(
        f'<rect x="{x - 40}" y="{y - 30}" width="80" height="60" rx="30" fill="url(#pal)" filter="url(#small)"/>'
        for x in xs
    )


def artwork(p):
    cx, cy, r = HEAD
    return f"""
    <g transform="translate(-6 -6)">
      <circle cx="{cx}" cy="{cy}" r="{r}" fill="url(#pal)"/>
      {eyes(p, cx, 366)}{cheeks(p, cx, 420)}
      <ellipse cx="460" cy="486" rx="200" ry="16" fill="#000" opacity=".28" filter="url(#soft)"/>
      <g filter="url(#drop)">
        <path d="{CUP_HANDLE}" fill="none" stroke="url(#cup)" stroke-width="48"/>
        <path d="{CUP_BODY}" fill="url(#cup)"/>
      </g>
      <path d="M270 482 H650" stroke="#fff" stroke-opacity=".45" stroke-width="6" stroke-linecap="round"/>
      <g fill="none" stroke="{p["inside"]}" stroke-width="22" stroke-linecap="round" stroke-linejoin="round">
        <path d="M404 596 L448 634 L404 672"/><path d="M478 672 H528"/>
      </g>
      {paws((356, 564))}
    </g>"""


def icon(p):
    margin = (1024 - TILE) / 2
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024">'
        + defs(p)
        + f'\n  <g transform="translate({margin:.0f} {margin:.0f}) scale({SCALE:.6f})">'
        + f'\n    <path d="{SHAPE}" fill="#000" filter="url(#tiledrop)"/>'
        + tile(p)
        + artwork(p)
        + "\n  </g>\n</svg>\n"
    )


if __name__ == "__main__":
    for name, palette in PALETTES.items():
        (OUT / f"cupper-{name}.svg").write_text(icon(palette), encoding="utf-8")
    print("wrote", ", ".join(f"cupper-{n}.svg" for n in PALETTES))
