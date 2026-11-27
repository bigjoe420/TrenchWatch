#!/usr/bin/env python3
"""Write assets/hero.svg — the TrenchWatch README hero banner (2560x1080).

The SVG is the source of truth; render it to PNG with VectorCraft:
  vectorcraft-cli convert assets/hero.svg assets/hero.png --ppi 144

Palette matches src/styles/global.css:
  bg #09090b, helius orange #ffa100, solana purple #9945ff, green #14f195.
"""

import math
from pathlib import Path

ROOT = Path(__file__).resolve().parent

ORANGE = "#ffa100"
PURPLE = "#9945ff"
GREEN = "#14f195"
FONT = "Cascadia Mono, Consolas, monospace"

# radar geometry (emblem uses unit R=184 around (256,266) in a 512 tile)
RADAR = {"cx": 1960, "cy": 540, "R": 400}
SWEEP_FROM, SWEEP_TO = -96, -18  # degrees from 3 o'clock, SVG y-down


def polar(cx, cy, r, deg):
    a = math.radians(deg)
    return cx + r * math.cos(a), cy + r * math.sin(a)


def rings(radar):
    cx, cy, R = radar["cx"], radar["cy"], radar["R"]
    parts = []
    for frac, op in ((0.25, 0.24), (0.5, 0.24), (0.75, 0.27), (1.0, 0.47)):
        r = R * frac
        parts.append(
            f'<circle cx="{cx}" cy="{cy}" r="{r:.1f}" fill="none" '
            f'stroke="#ffffff" stroke-opacity="{op}" stroke-width="2.5"/>'
        )
    return "\n    ".join(parts)


def ticks(radar):
    cx, cy, R = radar["cx"], radar["cy"], radar["R"]
    parts = []
    for i in range(12):
        deg = i * 30 - 90
        outer = R * (1.19 if i % 3 == 0 else 1.13)
        inner = R * 1.05
        x0, y0 = polar(cx, cy, inner, deg)
        x1, y1 = polar(cx, cy, outer, deg)
        op = "0.43" if i % 3 == 0 else "0.27"
        parts.append(
            f'<line x1="{x0:.1f}" y1="{y0:.1f}" x2="{x1:.1f}" y2="{y1:.1f}" '
            f'stroke="#ffffff" stroke-opacity="{op}" stroke-width="2.5"/>'
        )
    return "\n    ".join(parts)


def sweep(radar):
    cx, cy, R = radar["cx"], radar["cy"], radar["R"]
    x0, y0 = polar(cx, cy, R, SWEEP_FROM)
    x1, y1 = polar(cx, cy, R, SWEEP_TO)
    # launch trail: fading blips along the leading edge
    trail = []
    for frac, op, rad in ((0.45, 0.55, 7), (0.62, 0.4, 6), (0.78, 0.28, 5)):
        tx, ty = polar(cx, cy, R * frac, SWEEP_TO)
        trail.append(
            f'<circle cx="{tx:.1f}" cy="{ty:.1f}" r="{rad}" fill="#ffcd6e" fill-opacity="{op}"/>'
        )
    return f"""
    <path d="M {cx} {cy} L {x0:.1f} {y0:.1f} A {R} {R} 0 0 1 {x1:.1f} {y1:.1f} Z"
          fill="url(#sweepGrad)"/>
    {''.join(trail)}
    <line x1="{cx}" y1="{cy}" x2="{x1:.1f}" y2="{y1:.1f}" stroke="#ffcd6e"
          stroke-opacity="0.92" stroke-width="4" stroke-linecap="round"/>
    <circle cx="{x1:.1f}" cy="{y1:.1f}" r="18" fill="#ffdc96" fill-opacity="0.45"/>
    <circle cx="{x1:.1f}" cy="{y1:.1f}" r="9" fill="#fff0d2"/>"""


def blips(radar):
    cx, cy, R = radar["cx"], radar["cy"], radar["R"]
    out = []
    for deg, frac, ring_r, dot_r, color in (
        (-42, 0.62, 26, 11, GREEN),
        (215, 0.64, 22, 9, PURPLE),
    ):
        bx, by = polar(cx, cy, R * frac, deg)
        out.append(f"""
    <circle cx="{bx:.1f}" cy="{by:.1f}" r="{ring_r}" fill="none"
            stroke="{color}" stroke-opacity="0.43" stroke-width="3"/>
    <circle cx="{bx:.1f}" cy="{by:.1f}" r="{dot_r}" fill="{color}" fill-opacity="0.30"/>
    <circle cx="{bx:.1f}" cy="{by:.1f}" r="{dot_r * 0.55:.1f}" fill="#ffffff" fill-opacity="0.92"/>""")
    return "".join(out)


def crosshair(radar):
    cx, cy, R = radar["cx"], radar["cy"], radar["R"]
    arm = R * 0.08
    return f"""
    <g stroke="#ffffff" stroke-opacity="0.59" stroke-width="3">
      <line x1="{cx - arm:.1f}" y1="{cy}" x2="{cx + arm:.1f}" y2="{cy}"/>
      <line x1="{cx}" y1="{cy - arm:.1f}" x2="{cx}" y2="{cy + arm:.1f}"/>
    </g>
    <circle cx="{cx}" cy="{cy}" r="8" fill="#ffffff" fill-opacity="0.86"/>"""


def main():
    r = RADAR
    cx, cy = r["cx"], r["cy"]
    svg = f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 2560 1080" role="img" aria-label="TrenchWatch hero banner">
  <!--
    TrenchWatch hero banner. Regenerate after edits:
      python assets/generate_hero_svg.py
      vectorcraft-cli convert assets/hero.svg assets/hero.png (ppi 144)
  -->
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#101018"/>
      <stop offset="1" stop-color="#07070b"/>
    </linearGradient>
    <radialGradient id="radarGlow" cx="0.5" cy="0.5" r="0.5">
      <stop offset="0" stop-color="#ffa100" stop-opacity="0.10"/>
      <stop offset="0.7" stop-color="#ffa100" stop-opacity="0.03"/>
      <stop offset="1" stop-color="#ffa100" stop-opacity="0"/>
    </radialGradient>
    <radialGradient id="sweepGrad" cx="{cx}" cy="{cy}" r="{r['R']}"
                    gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="#ffa100" stop-opacity="0"/>
      <stop offset="0.55" stop-color="#ffa100" stop-opacity="0.30"/>
      <stop offset="1" stop-color="#ffa100" stop-opacity="0.62"/>
    </radialGradient>
    <linearGradient id="wordGrad" x1="0" y1="0" x2="1" y2="0">
      <stop offset="0" stop-color="{PURPLE}"/>
      <stop offset="1" stop-color="{GREEN}"/>
    </linearGradient>
    <pattern id="grid" width="64" height="64" patternUnits="userSpaceOnUse">
      <path d="M 64 0 L 0 0 0 64" fill="none" stroke="#ffffff"
            stroke-opacity="0.035" stroke-width="1"/>
    </pattern>
    <radialGradient id="vignette" cx="0.5" cy="0.5" r="0.75">
      <stop offset="0.55" stop-color="#000000" stop-opacity="0"/>
      <stop offset="1" stop-color="#000000" stop-opacity="0.45"/>
    </radialGradient>
  </defs>

  <rect width="2560" height="1080" fill="url(#bg)"/>
  <rect width="2560" height="1080" fill="url(#grid)"/>
  <circle cx="{cx}" cy="{cy}" r="720" fill="url(#radarGlow)"/>

  <g>
    {rings(r)}
    {ticks(r)}
    {sweep(r)}
    {blips(r)}
    {crosshair(r)}
  </g>

  <g font-family="{FONT}">
    <text x="170" y="500" font-size="148" font-weight="bold"
          letter-spacing="18" fill="#f7f7f8">TRENCH</text>
    <text x="170" y="500" font-size="148" font-weight="bold"
          letter-spacing="18" fill="url(#wordGrad)"
          transform="translate(790 0)">WATCH</text>
    <rect x="176" y="585" width="26" height="26" transform="rotate(45 189 598)"
          fill="{ORANGE}"/>
    <text x="240" y="607" font-size="44" letter-spacing="16" fill="#8b8b95">SOLANA LAUNCH MONITOR</text>
    <text x="176" y="700" font-size="34" letter-spacing="8" fill="#63636d">REAL-TIME RISK FLAGS · ON-CHAIN RECEIPTS · NO BLACK-BOX SCORES</text>
  </g>

  <rect width="2560" height="1080" fill="url(#vignette)"/>
</svg>
"""
    (ROOT / "hero.svg").write_text(svg, encoding="utf-8")
    print("wrote hero.svg")


if __name__ == "__main__":
    main()
