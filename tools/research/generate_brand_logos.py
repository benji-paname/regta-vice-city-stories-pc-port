"""Fixed ReGTA VCS wordmarks from authentic fonts. No browser or font embedding.
Generates:
  - docs/assets/logo.svg
  - docs/assets/logo-transparent.svg
  - docs/assets/logo-black.svg
  - docs/assets/logo-vcs.svg
  - docs/assets/logo-vcs-transparent.svg
  - docs/assets/logo-vcs-black.svg
  - docs/assets/favicon.svg
"""
from pathlib import Path
import argparse
from fontTools.ttLib import TTFont
from fontTools.pens.boundsPen import BoundsPen
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen

ROOT = Path(__file__).resolve().parents[2]
CTW_ROOT = Path("C:/Users/BENJAMIN/Documents/Codex/2026-10-17/tu-voit-le-projet-vcs-rust")

WHITE = '#fffdf5'
DARK = '#080808'
PINK = '#ff2a85'
CYAN = '#00f0ff'
GOLD = '#f2c35b'
PURPLE = '#7928ca'


def outline(font, text, cx, top, width, height, lower_e=False):
    glyphs = font.getGlyphSet()
    cmap = font.getBestCmap()
    placed = []
    advance = 0
    bounds = BoundsPen(glyphs)
    for ch in text:
        if ch == ' ':
            advance += font['hmtx']['space'][0] if 'space' in font['hmtx'].metrics else 400
            continue
        g = glyphs[cmap[ord(ch)]]
        k = 0.78 if lower_e and ch == 'e' else 1.0
        g.draw(TransformPen(bounds, (k, 0, 0, k, advance, 0)))
        placed.append((g, k, advance))
        advance += g.width * k
    x0, y0, x1, y1 = bounds.bounds
    s = min(width / (x1 - x0), height / (y1 - y0))
    left = cx - (x1 - x0) * s / 2
    pen = SVGPathPen(glyphs)
    for g, k, adv in placed:
        g.draw(TransformPen(pen, (s * k, 0, 0, -s * k, left + (adv - x0) * s, top + y1 * s)))
    return pen.getCommands()


def paint_regta(path):
    # Authentic GTA header wordmark: black drop shadow + clean black outline + off-white fill
    shadow = f'<path d="{path}" fill="{DARK}" stroke="{DARK}" stroke-width="20" stroke-linejoin="round" transform="translate(3 5)"/>'
    fill = f'<path d="{path}" fill="{WHITE}" stroke="{DARK}" stroke-width="7" paint-order="stroke fill" stroke-linejoin="round"/>'
    return shadow + fill


def paint_vice_city(path):
    # Iconic 80s Vice City neon script: black deep shadow + cyan/gold keyline + vibrant magenta/pink fill
    shadow = f'<path d="{path}" fill="{DARK}" stroke="{DARK}" stroke-width="16" stroke-linejoin="round" transform="translate(4 6)"/>'
    keyline = f'<path d="{path}" fill="{CYAN}" stroke="{CYAN}" stroke-width="8" stroke-linejoin="round"/>'
    fill = f'<path d="{path}" fill="{PINK}" stroke="{DARK}" stroke-width="3" paint-order="stroke fill" stroke-linejoin="round"/>'
    return shadow + keyline + fill


def paint_stories(path):
    # STORIES subtitle: black shadow + gold fill with crisp black outline
    shadow = f'<path d="{path}" fill="{DARK}" stroke="{DARK}" stroke-width="14" stroke-linejoin="round" transform="translate(2 4)"/>'
    keyline = f'<path d="{path}" fill="{DARK}" stroke="{DARK}" stroke-width="5" stroke-linejoin="round"/>'
    fill = f'<path d="{path}" fill="{GOLD}" stroke="{DARK}" stroke-width="2" paint-order="stroke fill" stroke-linejoin="round"/>'
    return shadow + keyline + fill


def paint_vcs_block(path):
    # Horizontal VCS lettering: dark shadow + cyan outline + pink fill
    shadow = f'<path d="{path}" fill="{DARK}" stroke="{DARK}" stroke-width="18" stroke-linejoin="round" transform="translate(3 5)"/>'
    keyline = f'<path d="{path}" fill="{CYAN}" stroke="{CYAN}" stroke-width="7" stroke-linejoin="round"/>'
    fill = f'<path d="{path}" fill="{PINK}"/>'
    return shadow + keyline + fill


def make_stacked(gta, rage):
    w, h = 900, 540
    regta_path = outline(gta, 'ReGTA', 450, 24, 760, 195, lower_e=True)
    vc_path = outline(rage, 'Vice City', 450, 245, 680, 140)
    stories_path = outline(gta, 'STORIES', 450, 410, 410, 60)

    paths = [
        paint_regta(regta_path),
        paint_vice_city(vc_path),
        paint_stories(stories_path)
    ]
    baseline = 512

    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}" role="img">
<title>ReGTA VCS — PC Recomp</title><desc>Independent project wordmark. Pricedown Black with a reduced lowercase e; Vice City script in neon magenta with cyan keyline and gold Stories banner. Not an official Rockstar logo.</desc>
{''.join(paths)}
<text x="{w/2}" y="{baseline}" text-anchor="middle" fill="{WHITE}" font-family="Arial,sans-serif" font-size="22" letter-spacing="6">PC RECOMP</text>
</svg>'''


def make_horizontal(gta, rage):
    w, h = 1200, 280
    regta_path = outline(gta, 'ReGTA', 370, 24, 680, 175, lower_e=True)
    vc_path = outline(rage, 'Vice City', 960, 22, 420, 95)
    stories_path = outline(gta, 'STORIES', 960, 130, 290, 52)

    paths = [
        paint_regta(regta_path),
        paint_vice_city(vc_path),
        paint_stories(stories_path)
    ]
    baseline = 257

    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}" role="img">
<title>ReGTA VCS — PC Recomp</title><desc>Independent project wordmark. Pricedown Black with a reduced lowercase e; Vice City script in neon magenta and Stories banner. Not an official Rockstar logo.</desc>
{''.join(paths)}
<text x="{w/2}" y="{baseline}" text-anchor="middle" fill="{WHITE}" font-family="Arial,sans-serif" font-size="22" letter-spacing="6">PC RECOMP</text>
</svg>'''


def make_favicon():
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><title>ReGTA VCS</title><rect width="64" height="64" rx="12" fill="#0b0d11"/><path d="M16 13h18c12 0 18 6 18 15 0 7-4 12-10 14l11 10H38L27 40v12H16zm11 10v8h8c4 0 6-1 6-4s-2-4-6-4z" fill="{PINK}"/><path d="M13 56h40" stroke="{CYAN}" stroke-width="3"/></svg>'''


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--pricedown', type=Path, default=CTW_ROOT / 'target/private-fonts/pricedown/Pricedown Bl.otf')
    ap.add_argument('--rage', type=Path, default=Path('C:/Windows/Fonts/RAGE.TTF'))
    ap.add_argument('--output', type=Path, default=ROOT / 'docs/assets')
    a = ap.parse_args()

    gta = TTFont(a.pricedown)
    rage = TTFont(a.rage)
    a.output.mkdir(parents=True, exist_ok=True)

    # 1. Stacked logo
    stacked = make_stacked(gta, rage)
    for suffix in ['', '-transparent']:
        (a.output / f'logo{suffix}.svg').write_text(stacked, encoding='utf-8')
    black_stacked = stacked.replace('<title>', '<rect width="100%" height="100%" fill="#000000"/><title>', 1)
    (a.output / 'logo-black.svg').write_text(black_stacked, encoding='utf-8')

    # 2. Horizontal logo
    horizontal = make_horizontal(gta, rage)
    for suffix in ['', '-transparent']:
        (a.output / f'logo-vcs{suffix}.svg').write_text(horizontal, encoding='utf-8')
    black_horizontal = horizontal.replace('<title>', '<rect width="100%" height="100%" fill="#000000"/><title>', 1)
    (a.output / 'logo-vcs-black.svg').write_text(black_horizontal, encoding='utf-8')

    # 3. Favicon
    (a.output / 'favicon.svg').write_text(make_favicon(), encoding='utf-8')

    print("[OK] Brand logos and favicon generated successfully in docs/assets/")


if __name__ == '__main__':
    main()
