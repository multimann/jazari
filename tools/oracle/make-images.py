"""Generate the source images for the bitmaps fixture (needs Pillow).

usage: python make-images.py [outdir]   (default: tests/fixtures/_src of this repository)
  alpha-disc.png  128x128 RGBA: four opaque colour quadrants inside a circle,
                  a 50%-alpha white ring, a soft black alpha falloff, and fully
                  transparent corners.
  bars.jpg        160x120 RGB, quality 85: eight colour bars over a vertical
                  brightness ramp, a 3 px black frame and a brown disc.
"""
import math
import os
import sys

from PIL import Image, ImageDraw


def main(out):
    os.makedirs(out, exist_ok=True)
    im = Image.new('RGBA', (128, 128), (0, 0, 0, 0))
    px = im.load()
    c = 63.5
    for y in range(128):
        for x in range(128):
            r = math.hypot(x - c, y - c)
            if r <= 44:
                q = (x >= 64) + 2 * (y >= 64)
                px[x, y] = [(230, 40, 40), (40, 180, 60), (40, 90, 230), (250, 200, 30)][q] + (255,)
            elif r <= 54:
                px[x, y] = (255, 255, 255, 128)
            elif r <= 62:
                px[x, y] = (0, 0, 0, int(255 * (62 - r) / 8))
    im.save(os.path.join(out, 'alpha-disc.png'))

    j = Image.new('RGB', (160, 120))
    d = j.load()
    bars = [(255, 255, 255), (255, 255, 0), (0, 255, 255), (0, 255, 0), (255, 0, 255), (255, 0, 0), (0, 0, 255), (0, 0, 0)]
    for y in range(120):
        k = 1 - y / 180
        for x in range(160):
            d[x, y] = tuple(int(v * k) for v in bars[x // 20])
    dr = ImageDraw.Draw(j)
    dr.rectangle([0, 0, 159, 119], outline=(0, 0, 0), width=3)
    dr.ellipse([60, 40, 100, 80], fill=(128, 64, 0))
    j.save(os.path.join(out, 'bars.jpg'), quality=85)
    print('wrote', out)


if __name__ == '__main__':
    main(sys.argv[1] if len(sys.argv) > 1 else
         os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..', 'tests', 'fixtures', '_src'))
