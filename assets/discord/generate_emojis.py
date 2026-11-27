"""Generate the TrenchWatch Discord server icon + custom emoji set."""
from PIL import Image, ImageDraw
import os

OUT = os.path.dirname(os.path.abspath(__file__))
ICON = r"C:\Development\TrenchWatch\assets\icon.png"

HEL = (255, 161, 0)      # helius orange (brand, per in-app palette)
PUR = (153, 69, 255)     # solana purple
GRN = (20, 241, 149)     # solana green
RED = (255, 77, 109)
DARK = (16, 16, 20)

def canvas(size=128, scale=4):
    img = Image.new("RGBA", (size * scale, size * scale), (0, 0, 0, 0))
    return img, ImageDraw.Draw(img), scale

def finish(img, size=128, name=""):
    img = img.resize((size, size), Image.LANCZOS)
    img.save(os.path.join(OUT, name), optimize=True)
    print("wrote", name)

# --- server icon: the app emblem, 512px -----------------------------------
icon = Image.open(ICON).convert("RGBA").resize((512, 512), Image.LANCZOS)
icon.save(os.path.join(OUT, "trenchwatch-server-icon.png"), optimize=True)
print("wrote trenchwatch-server-icon.png")

# --- emoji: trenchwatch emblem ---------------------------------------------
emb = Image.open(ICON).convert("RGBA")
# crop the central radar (skip the tile border) for a tighter emoji read
w, h = emb.size
m = int(w * 0.16)
emb = emb.crop((m, m, w - m, h - m)).resize((128, 128), Image.LANCZOS)
emb.save(os.path.join(OUT, "emoji-trenchwatch.png"), optimize=True)
print("wrote emoji-trenchwatch.png")

# --- emoji: helius (orange H mark on dark rounded tile) ---------------------
img, d, s = canvas()
def rr(xy, r, **kw): d.rounded_rectangle([v * s for v in xy], radius=r * s, **kw)
rr((8, 8, 120, 120), 28, fill=DARK)
# H with a forward slash energy: two stems + crossbar, orange
for x in (32, 76):
    rr((x, 30, x + 20, 98), 9, fill=HEL)
rr((32, 58, 96, 74), 8, fill=HEL)
# spark dot above-right, like a ping
d.ellipse([v * s for v in (96, 22, 112, 38)], fill=HEL)
finish(img, name="emoji-helius.png")

# --- emoji: graduation cap (solana green) -----------------------------------
img, d, s = canvas()
def poly(pts, **kw): d.polygon([(x * s, y * s) for x, y in pts], **kw)
poly([(64, 22), (116, 48), (64, 74), (12, 48)], fill=GRN)          # mortarboard
d.pieslice([v * s for v in (30, 46, 98, 106)], 0, 180, fill=GRN)   # cap head
d.line([v * s for v in (100, 52, 100, 92)], fill=GRN, width=5 * s) # tassel
d.ellipse([v * s for v in (94, 88, 106, 100)], fill=GRN)
finish(img, name="emoji-graduated.png")

# --- emoji: critical (red diamond + bolt) -----------------------------------
img, d, s = canvas()
poly([(64, 10), (118, 64), (64, 118), (10, 64)], fill=RED)
poly([(70, 34), (52, 68), (63, 68), (58, 94), (78, 58), (66, 58)], fill=(255, 255, 255))
finish(img, name="emoji-critical.png")

# --- emoji: warn (orange triangle + !) --------------------------------------
img, d, s = canvas()
poly([(64, 14), (116, 106), (12, 106)], fill=HEL)
d.rounded_rectangle([61 * s, 44 * s, 67 * s, 74 * s], radius=3 * s, fill=DARK)
d.ellipse([61 * s, 84 * s, 67 * s, 90 * s], fill=DARK)
finish(img, name="emoji-warn.png")

# --- emoji: radar (sweep + blip) --------------------------------------------
img, d, s = canvas()
cx, cy, R = 64, 64, 46
for r in (16, 30, R):
    d.ellipse([(cx - r) * s, (cy - r) * s, (cx + r) * s, (cy + r) * s],
              outline=GRN, width=4 * s)
d.pieslice([(cx - R) * s, (cy - R) * s, (cx + R) * s, (cy + R) * s],
           start=-90, end=-10, fill=GRN)
d.ellipse([(cx - 7) * s, (cy - 7) * s, (cx + 7) * s, (cy + 7) * s], fill=GRN)
d.ellipse([(cx + 24) * s, (cy - 30) * s, (cx + 32) * s, (cy - 22) * s], fill=(255, 255, 255))
finish(img, name="emoji-radar.png")

# --- emoji: receipt (scroll with check) -------------------------------------
img, d, s = canvas()
rr = lambda xy, r, **kw: d.rounded_rectangle([v * s for v in xy], radius=r * s, **kw)
rr((30, 14, 98, 114), 8, fill=(240, 240, 245))
for y in (36, 50, 64):
    d.line([v * s for v in (42, y, 86, y)], fill=(120, 120, 130), width=5 * s)
d.line([v * s for v in (46, 88, 58, 100)], fill=GRN, width=7 * s)
d.line([v * s for v in (58, 100, 88, 66)], fill=GRN, width=7 * s)
finish(img, name="emoji-receipt.png")

# --- emoji: pump (curve completing — purple->green arrow) -------------------
img, d, s = canvas()
d.arc([16 * s, 20 * s, 112 * s, 116 * s], start=100, end=355, fill=PUR, width=8 * s)
poly([(92, 18), (118, 44), (100, 44), (100, 62)], fill=GRN)
finish(img, name="emoji-pump.png")

print("all done")
