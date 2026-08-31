"""Draw web/static/og.png, the picture a chat app or a social network shows
under a pasted link to the site.

    python tools/og_image.py

1200 x 630 is the size they all crop to. Needs Pillow and the Noto Sans
fonts; run it again after changing the icon or the wording.
"""
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
FONTS = Path('/usr/share/fonts/noto')
BG, INK, MUTED, ACCENT = '#0d0e10', '#e8e9ea', '#8d939c', '#e5a03c'

card = Image.new('RGB', (1200, 630), BG)
icon = Image.open(ROOT / 'web/static/icon-512.png').convert('RGB').resize((300, 300), Image.LANCZOS)
card.paste(icon, (90, 165))

draw = ImageDraw.Draw(card)
name = ImageFont.truetype(str(FONTS / 'NotoSans-SemiBold.ttf'), 104)
line = ImageFont.truetype(str(FONTS / 'NotoSans-Regular.ttf'), 40)
fine = ImageFont.truetype(str(FONTS / 'NotoSans-Regular.ttf'), 30)

x = 450
draw.text((x, 170), 'LumiRaw', font=name, fill=INK)
draw.text((x, 318), 'Raw photos, developed', font=line, fill=INK)
draw.text((x, 368), 'in your browser.', font=line, fill=INK)
draw.text((x, 440), 'Nothing is uploaded.', font=fine, fill=MUTED)
draw.rectangle((x, 300, x + 64, 304), fill=ACCENT)

out = ROOT / 'web/static/og.png'
card.save(out, optimize=True)
print(out)
