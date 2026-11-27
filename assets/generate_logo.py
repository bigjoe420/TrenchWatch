#!/usr/bin/env python3
"""Regenerate the TrenchWatch logo assets.

Outputs (all RGBA, 4x supersampled then Lanczos-downscaled):
  assets/icon.png       square app icon (emblem only), 1024
  assets/logo-dark.png  horizontal lockup for dark UIs, 2560x768
  assets/logo-light.png same lockup recolored for light UIs

Palette matches src/styles/global.css:
  bg #09090b, elevated #101013, helius orange #ffa100,
  solana purple #9945ff, solana green #14f195, text #fafafa.

The SVG in this directory is the hand-maintained source of the emblem;
this script is the source of the raster assets. Run:
  python assets/generate_logo.py
"""

import math
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parent
FONT = r"C:\Windows\Fonts\CascadiaMono.ttf"

ORANGE = (255, 161, 0)
PURPLE = (153, 69, 255)
GREEN = (20, 241, 149)
SUPER = 4  # supersample factor


def lerp(a, b, t):
    return tuple(int(round(a[i] + (b[i] - a[i]) * t)) for i in range(3))


def vertical_gradient(size, top, bottom):
    """Vertical RGB gradient as an Image."""
    w, h = size
    t = np.linspace(0.0, 1.0, h, dtype=np.float32)[:, None, None]
    top_a = np.array(top, dtype=np.float32)[None, None, :]
    bot_a = np.array(bottom, dtype=np.float32)[None, None, :]
    arr = top_a + (bot_a - top_a) * t
    return Image.fromarray(np.repeat(arr, w, axis=1).astype(np.uint8), "RGB")


def radial_alpha_mask(size, center, radius, exponent=1.5, peak=165):
    """L-mode mask: alpha = peak * (r/radius)^exponent inside radius."""
    w, h = size
    y, x = np.mgrid[0:h, 0:w].astype(np.float32)
    dist = np.sqrt((x - center[0]) ** 2 + (y - center[1]) ** 2) / radius
    alpha = np.clip(dist, 0.0, 1.0) ** exponent
    alpha[dist > 1.0] = 0.0
    return Image.fromarray((alpha * peak).astype(np.uint8), "L")


def draw_emblem(n):
    """Radar emblem on a dark rounded tile. Returns RGBA image of n x n."""
    n *= SUPER
    img = Image.new("RGBA", (n, n), (0, 0, 0, 0))

    # --- tile -------------------------------------------------------------
    margin = int(n * 0.02)
    radius = int(n * 0.235)
    tile_box = [margin, margin, n - margin, n - margin]
    tile = vertical_gradient((n, n), (21, 21, 28), (10, 10, 14)).convert("RGBA")
    tile_mask = Image.new("L", (n, n), 0)
    ImageDraw.Draw(tile_mask).rounded_rectangle(tile_box, radius, fill=255)
    img.paste(tile, (0, 0), tile_mask)

    # faint inner edge highlight
    edge = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    ImageDraw.Draw(edge).rounded_rectangle(
        tile_box, radius, outline=(255, 255, 255, 20), width=max(2, n // 512)
    )
    img.alpha_composite(edge)

    cx, cy = n * 0.5, n * 0.52
    R = n * 0.36

    # --- rings ------------------------------------------------------------
    rings = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    rd = ImageDraw.Draw(rings)
    for frac, alpha in ((0.25, 60), (0.5, 60), (0.75, 70), (1.0, 120)):
        r = R * frac
        rd.ellipse(
            [cx - r, cy - r, cx + r, cy + r],
            outline=(255, 255, 255, alpha),
            width=max(2, int(n * 0.0035)),
        )

    # --- ticks ------------------------------------------------------------
    for i in range(12):
        ang = math.radians(i * 30 - 90)
        outer = R * (1.18 if i % 3 == 0 else 1.13)
        inner = R * 1.05
        x0, y0 = cx + inner * math.cos(ang), cy + inner * math.sin(ang)
        x1, y1 = cx + outer * math.cos(ang), cy + outer * math.sin(ang)
        rd.line(
            [x0, y0, x1, y1],
            fill=(255, 255, 255, 110 if i % 3 == 0 else 70),
            width=max(2, int(n * 0.0035)),
        )
    img.alpha_composite(rings)

    # --- sweep ------------------------------------------------------------
    sweep_from, sweep_to = -90, -90 + 70
    wedge_mask = Image.new("L", (n, n), 0)
    ImageDraw.Draw(wedge_mask).pieslice(
        [cx - R, cy - R, cx + R, cy + R], sweep_from, sweep_to, fill=255
    )
    grad = radial_alpha_mask((n, n), (cx, cy), R, exponent=1.6, peak=150)
    sweep_alpha = Image.composite(grad, Image.new("L", (n, n), 0), wedge_mask)
    sweep = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    sweep.putalpha(sweep_alpha)
    sweep_col = Image.new("RGBA", (n, n), ORANGE + (255,))
    img.paste(sweep_col, (0, 0), sweep)

    # sweep leading edge: glow + sharp line + ping dot
    edge_ang = math.radians(sweep_to)
    ex, ey = cx + R * math.cos(edge_ang), cy + R * math.sin(edge_ang)
    glow = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    gd = ImageDraw.Draw(glow)
    gd.line([cx, cy, ex, ey], fill=ORANGE + (200,), width=max(3, int(n * 0.010)))
    glow = glow.filter(ImageFilter.GaussianBlur(n * 0.012))
    img.alpha_composite(glow)
    sharp = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    ImageDraw.Draw(sharp).line(
        [cx, cy, ex, ey], fill=(255, 205, 110, 235), width=max(2, int(n * 0.0045))
    )
    img.alpha_composite(sharp)
    ping = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    pd = ImageDraw.Draw(ping)
    pd.ellipse(
        [ex - n * 0.014, ey - n * 0.014, ex + n * 0.014, ey + n * 0.014],
        fill=(255, 220, 140, 255),
    )
    ping = ping.filter(ImageFilter.GaussianBlur(n * 0.006))
    img.alpha_composite(ping)
    pd2 = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    ImageDraw.Draw(pd2).ellipse(
        [ex - n * 0.009, ey - n * 0.009, ex + n * 0.009, ey + n * 0.009],
        fill=(255, 240, 210, 255),
    )
    img.alpha_composite(pd2)

    # --- blips (detected launches) ----------------------------------------
    for ang_deg, frac, size_frac, color in (
        (28, 0.56, 0.016, GREEN),
        (163, 0.74, 0.013, PURPLE),
    ):
        ang = math.radians(ang_deg - 90)
        bx, by = cx + R * frac * math.cos(ang), cy + R * frac * math.sin(ang)
        blip = Image.new("RGBA", (n, n), (0, 0, 0, 0))
        bd = ImageDraw.Draw(blip)
        bd.ellipse(
            [bx - n * size_frac, by - n * size_frac, bx + n * size_frac, by + n * size_frac],
            fill=color + (255,),
        )
        blip = blip.filter(ImageFilter.GaussianBlur(n * 0.005))
        img.alpha_composite(blip)
        core = Image.new("RGBA", (n, n), (0, 0, 0, 0))
        ImageDraw.Draw(core).ellipse(
            [bx - n * size_frac * 0.55, by - n * size_frac * 0.55,
             bx + n * size_frac * 0.55, by + n * size_frac * 0.55],
            fill=(255, 255, 255, 235),
        )
        img.alpha_composite(core)
        ring_r = n * (size_frac + 0.022)
        ring = Image.new("RGBA", (n, n), (0, 0, 0, 0))
        ImageDraw.Draw(ring).ellipse(
            [bx - ring_r, by - ring_r, bx + ring_r, by + ring_r],
            outline=color + (110,),
            width=max(2, int(n * 0.0035)),
        )
        img.alpha_composite(ring)

    # --- center crosshair ---------------------------------------------------
    cross = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    cd = ImageDraw.Draw(cross)
    arm = R * 0.08
    cd.line([cx - arm, cy, cx + arm, cy], fill=(255, 255, 255, 150), width=max(2, int(n * 0.0035)))
    cd.line([cx, cy - arm, cx, cy + arm], fill=(255, 255, 255, 150), width=max(2, int(n * 0.0035)))
    cd.ellipse(
        [cx - n * 0.008, cy - n * 0.008, cx + n * 0.008, cy + n * 0.008],
        fill=(255, 255, 255, 220),
    )
    img.alpha_composite(cross)

    return img.resize((n // SUPER, n // SUPER), Image.LANCZOS)


def spaced_text(draw, xy, text, font, fill, tracking, stroke=0, stroke_fill=None):
    """Draw text with manual letterspacing; returns absolute x after the
    last glyph (no trailing tracking)."""
    x, y = xy
    for ch in text:
        draw.text(
            (x, y), ch, font=font, fill=fill,
            stroke_width=stroke, stroke_fill=stroke_fill or fill,
        )
        x += draw.textlength(ch, font=font) + tracking
    return x - tracking


def text_width(draw, text, font, tracking):
    width = sum(draw.textlength(ch, font=font) for ch in text)
    return width + tracking * (len(text) - 1)


def make_lockup(emblem, dark, w=2700, h=768):
    """Emblem + wordmark + tagline on transparent background."""
    w_s, h_s = w * SUPER, h * SUPER
    img = Image.new("RGBA", (w_s, h_s), (0, 0, 0, 0))

    # emblem, vertically centered
    e_size = int(h_s * 0.84)
    emb = emblem.resize((e_size, e_size), Image.LANCZOS)
    img.alpha_composite(emb, (int(h_s * 0.10), (h_s - e_size) // 2))

    draw = ImageDraw.Draw(img)
    main_color = (247, 247, 248) if dark else (16, 16, 20)
    dim_color = (139, 139, 149) if dark else (85, 85, 94)

    # wordmark: TRENCH (solid) + WATCH (solana gradient)
    font_size = int(h_s * 0.28)
    font = ImageFont.truetype(FONT, font_size)
    tracking = font_size * 0.14
    x0 = int(h_s * 0.10) + e_size + int(h_s * 0.14)
    y0 = int(h_s * 0.28)

    trench = "TRENCH"
    watch = "WATCH"
    stroke = max(2, int(SUPER * 1.2))

    draw_y = y0 - int(font_size * 0.22)  # font bbox top offset
    x_end = spaced_text(
        draw, (x0, draw_y), trench, font, main_color, tracking,
        stroke=stroke, stroke_fill=main_color,
    )
    watch_x = x_end + tracking * 1.35

    # gradient text via mask
    watch_w = int(text_width(draw, watch, font, tracking)) + stroke * 2
    mask = Image.new("L", (watch_w, font_size * 2), 0)
    md = ImageDraw.Draw(mask)
    spaced_text(
        md, (stroke, stroke), watch, font, 255, tracking, stroke=stroke, stroke_fill=255
    )
    t = np.linspace(0.0, 1.0, watch_w, dtype=np.float32)[None, :, None]
    grad = (
        np.array(PURPLE, dtype=np.float32)[None, None, :] * (1 - t)
        + np.array(GREEN, dtype=np.float32)[None, None, :] * t
    )
    grad_img = Image.fromarray(
        np.repeat(grad, mask.height, axis=0).astype(np.uint8), "RGB"
    ).convert("RGBA")
    img.paste(grad_img, (int(watch_x - stroke), int(draw_y - stroke)), mask)

    # tagline with orange diamond bullet
    tag_size = int(h_s * 0.088)
    tag_font = ImageFont.truetype(FONT, tag_size)
    tag_tracking = tag_size * 0.38
    tag = "SOLANA LAUNCH MONITOR"
    tag_y = y0 + int(font_size * 1.18)
    d_size = int(tag_size * 0.42)
    dy = tag_y + int(tag_size * 0.62)  # vertical center of the cap line
    cx_d = x0 + d_size
    draw.polygon(
        [(cx_d, dy - d_size), (cx_d + d_size, dy), (cx_d, dy + d_size), (cx_d - d_size, dy)],
        fill=ORANGE + (255,),
    )
    spaced_text(
        draw, (x0 + d_size * 3.2, tag_y), tag, tag_font, dim_color,
        tag_tracking, stroke=max(1, SUPER // 2), stroke_fill=dim_color,
    )

    return img.resize((w, h), Image.LANCZOS)


def main():
    emblem = draw_emblem(1024)
    icon = emblem.resize((1024, 1024), Image.LANCZOS)
    icon.save(ROOT / "icon.png")
    make_lockup(emblem, dark=True).save(ROOT / "logo-dark.png")
    make_lockup(emblem, dark=False).save(ROOT / "logo-light.png")
    print("wrote icon.png, logo-dark.png, logo-light.png")


if __name__ == "__main__":
    main()
