"""Compact, human-readable summary of an XFL folder (as written by JPEXS FLA Convert).

usage: python xfl-summary.py tests/fixtures/NAME/xfl

For the document and every LIBRARY/*.xml symbol: timelines, layers, frames
(index, duration, tween settings, labels, scripts) and elements (shapes with
fill/stroke style summaries, symbol instances with name / matrix / colour /
filters / blend / cache / loop / firstFrame, text runs, bitmaps).
"""
import os
import re
import sys
import xml.etree.ElementTree as ET

NS = '{http://ns.adobe.com/xfl/2008/}'


def tag(e):
    return e.tag.replace(NS, '')


def kids(e, name):
    return [c for c in e if tag(c) == name]


def find(e, path):
    for p in path.split('/'):
        if e is None:
            return None
        nxt = [c for c in e if tag(c) == p]
        e = nxt[0] if nxt else None
    return e


def matrix(e):
    m = find(e, 'matrix/Matrix')
    if m is None:
        return 'identity'
    a = {k: float(m.get(k, d)) for k, d in (('a', 1), ('b', 0), ('c', 0), ('d', 1), ('tx', 0), ('ty', 0))}
    return 'm(a=%g b=%g c=%g d=%g tx=%g ty=%g)' % (a['a'], a['b'], a['c'], a['d'], a['tx'], a['ty'])


def fill(e):
    t = tag(e)
    if t == 'SolidColor':
        return 'solid %s a=%s' % (e.get('color', '#000000'), e.get('alpha', '1'))
    if t in ('LinearGradient', 'RadialGradient'):
        stops = ' '.join('%s:%s/%s' % (round(float(g.get('ratio', 0)) * 255), g.get('color', '#000000'), g.get('alpha', '1'))
                         for g in kids(e, 'GradientEntry'))
        extra = ''
        if t == 'RadialGradient':
            extra = ' focal=%s' % e.get('focalPointRatio', '0')
        return '%s spread=%s interp=%s%s %s [%s]' % (t, e.get('spreadMethod', 'pad'), e.get('interpolationMethod', 'rgb'),
                                                   extra, matrix(e), stops)
    if t == 'BitmapFill':
        return 'bitmap %s clipped=%s smoothed=%s %s' % (e.get('bitmapPath'), e.get('bitmapIsClipped'), e.get('allowSmoothing'), matrix(e))
    return t


def shape(e, ind, out):
    fills = [(f.get('index'), fill(list(f)[0])) for f in kids(find(e, 'fills') or ET.Element('x'), 'FillStyle')]
    strokes = []
    for s in kids(find(e, 'strokes') or ET.Element('x'), 'StrokeStyle'):
        st = list(s)[0]
        f = find(st, 'fill')
        attrs = ' '.join('%s=%s' % (k, v) for k, v in sorted(st.attrib.items()))
        strokes.append((s.get('index'), '%s %s fill=%s' % (tag(st), attrs, fill(list(f)[0]) if f is not None and len(f) else '?')))
    edges = kids(find(e, 'edges') or ET.Element('x'), 'Edge')
    flags = ''.join(' %s=%s' % (k, e.get(k)) for k in ('isDrawingObject', 'isFloating') if k in e.attrib)
    out.append('%s%s %s edges=%d%s' % (ind, 'DOMShape' if tag(e) == 'DOMShape' else tag(e), matrix(e), len(edges), flags))
    for i, f in fills:
        out.append('%s    fill %s: %s' % (ind, i, f))
    for i, s in strokes:
        out.append('%s    stroke %s: %s' % (ind, i, s))
    for key, (x0, y0, x1, y1) in sorted(edge_bounds(edges).items(), key=lambda kv: (kv[0].rstrip('0123456789'), int(kv[0].lstrip('fillstroke')))):
        out.append('%s    bounds %s = (%g,%g)-(%g,%g)' % (ind, key, x0 / 20.0, y0 / 20.0, x1 / 20.0, y1 / 20.0))


EDGE_TOKEN = re.compile(r'[!|/\[\]]|S\d+|#?-?[0-9A-Fa-f]*\.?[0-9A-Fa-f]+')


def edge_number(t):
    """An XFL edge coordinate in twips: decimal, or '#' hex with an optional hex fraction."""
    if t.startswith('#'):
        t = t[1:]
        neg = t.startswith('-')
        t = t.lstrip('-')
        whole, _, frac = t.partition('.')
        v = int(whole or '0', 16) + (int(frac, 16) / 16.0 ** len(frac) if frac else 0.0)
        return -v if neg else v
    return float(t)


def edge_bounds(edges):
    """Bounds (twips, shape-local) of the edges that use each fill and stroke style."""
    res = {}
    for ed in edges:
        keys = ['fill%s' % ed.get(k) for k in ('fillStyle0', 'fillStyle1') if ed.get(k) not in (None, '', '0')]
        if ed.get('strokeStyle') not in (None, '', '0'):
            keys.append('stroke%s' % ed.get('strokeStyle'))
        if not keys or not ed.get('edges'):
            continue
        toks = EDGE_TOKEN.findall(ed.get('edges'))
        pts, nums, cmd = [], [], None
        for t in toks + ['!']:
            if t in '!|/[]' or t.startswith('S'):
                if cmd is not None:
                    pts.extend(zip(nums[0::2], nums[1::2]))
                cmd, nums = (None if t.startswith('S') else t), []
            else:
                nums.append(edge_number(t))
        for k in keys:
            for x, y in pts:
                b = res.setdefault(k, [x, y, x, y])
                b[0] = min(b[0], x); b[1] = min(b[1], y); b[2] = max(b[2], x); b[3] = max(b[3], y)
    return res


def element(e, ind, out):
    t = tag(e)
    if t in ('DOMShape', 'DOMRectangleObject', 'DOMOvalObject'):
        shape(e, ind, out)
    elif t == 'DOMSymbolInstance':
        a = e.attrib
        s = '%sDOMSymbolInstance %s type=%s' % (ind, a.get('libraryItemName'), a.get('symbolType', 'movie clip'))
        for k in ('name', 'loop', 'firstFrame', 'blendMode', 'cacheAsBitmap', 'isVisible'):
            if k in a:
                s += ' %s=%r' % (k, a[k])
        s += ' ' + matrix(e)
        out.append(s)
        tp = find(e, 'transformationPoint/Point')
        if tp is not None:
            out.append('%s    transformationPoint=(%s,%s)' % (ind, tp.get('x', 0), tp.get('y', 0)))
        c = find(e, 'color/Color')
        if c is not None:
            out.append('%s    color %s' % (ind, ' '.join('%s=%s' % kv for kv in sorted(c.attrib.items()))))
        fl = find(e, 'filters')
        if fl is not None:
            for f in fl:
                out.append('%s    filter %s %s' % (ind, tag(f), ' '.join('%s=%s' % kv for kv in sorted(f.attrib.items()))))
        sc = find(e, 'Actionscript/script')
        if sc is not None:
            out.append('%s    script: %r' % (ind, sc.text))
    elif t in ('DOMStaticText', 'DOMDynamicText', 'DOMInputText'):
        a = e.attrib
        s = '%s%s %s' % (ind, t, ' '.join('%s=%r' % (k, a[k]) for k in sorted(a) if k in (
            'name', 'variableName', 'width', 'height', 'left', 'lineType', 'autoExpand', 'isSelectable', 'fontRenderingMode')))
        out.append(s + ' ' + matrix(e))
        for run in kids(find(e, 'textRuns') or ET.Element('x'), 'DOMTextRun'):
            chars = find(run, 'characters')
            ta = find(run, 'textAttrs/DOMTextAttrs')
            txt = chars.text if chars is not None else ''
            attrs = ' '.join('%s=%s' % kv for kv in sorted(ta.attrib.items())) if ta is not None else ''
            out.append('%s    run %r codes=[%s] %s' % (ind, txt, ' '.join('U+%04X' % ord(ch) for ch in (txt or '') if ord(ch) > 127), attrs))
    elif t == 'DOMBitmapInstance':
        out.append('%sDOMBitmapInstance %s %s' % (ind, e.get('libraryItemName'), matrix(e)))
    elif t == 'DOMGroup':
        s = '%sDOMGroup %s' % (ind, matrix(e) if find(e, 'matrix/Matrix') is not None else '(no matrix)')
        tp = find(e, 'transformationPoint/Point')
        if tp is not None:
            s += ' transformationPoint=(%s,%s)' % (tp.get('x', 0), tp.get('y', 0))
        out.append(s)
        members = find(e, 'members')
        for m in (list(members) if members is not None else []):
            element(m, ind + '    ', out)
    else:
        out.append('%s%s %s' % (ind, t, dict(e.attrib)))


def timeline(tl, ind, out):
    out.append('%sDOMTimeline %r' % (ind, tl.get('name')))
    for layer in kids(find(tl, 'layers'), 'DOMLayer'):
        a = layer.attrib
        frames = kids(find(layer, 'frames') or ET.Element('x'), 'DOMFrame')
        total = sum(int(f.get('duration', 1)) for f in frames)
        extra = ''.join(' %s=%s' % (k, a[k]) for k in ('parentLayerIndex', 'visible', 'locked') if k in a)
        out.append('%s  layer %r type=%s frames=%d%s' % (ind, a.get('name'), a.get('layerType', 'normal'), total, extra))
        for f in frames:
            fa = f.attrib
            s = '%s    frame %s dur=%s' % (ind, fa.get('index'), fa.get('duration', '1'))
            for k in ('name', 'labelType', 'tweenType', 'motionTweenRotate', 'motionTweenRotateTimes', 'acceleration',
                      'motionTweenScale', 'keyMode', 'useSingleEaseCurve', 'hasCustomEase'):
                if k in fa:
                    s += ' %s=%s' % (k, fa[k])
            out.append(s)
            sc = find(f, 'Actionscript/script')
            if sc is not None:
                out.append('%s      script: %r' % (ind, sc.text))
            els = find(f, 'elements')
            for el in (list(els) if els is not None else []):
                element(el, ind + '      ', out)


def main(d):
    out = []
    doc = ET.parse(os.path.join(d, 'DOMDocument.xml')).getroot()
    a = doc.attrib
    out.append('DOMDocument width=%s height=%s frameRate=%s backgroundColor=%s xflVersion=%s' % (
        a.get('width', 550), a.get('height', 400), a.get('frameRate', 24), a.get('backgroundColor', '#FFFFFF'),
        a.get('xflVersion')))
    media = find(doc, 'media')
    for m in (media if media is not None else []):
        out.append('  media %s %s' % (tag(m), ' '.join('%s=%r' % kv for kv in sorted(m.attrib.items()))))
    for inc in (find(doc, 'symbols') or []):
        out.append('  symbol include %s' % inc.get('href'))
    for tl in kids(find(doc, 'timelines'), 'DOMTimeline'):
        timeline(tl, '', out)
    lib = os.path.join(d, 'LIBRARY')
    if os.path.isdir(lib):
        for fn in sorted(os.listdir(lib)):
            if not fn.endswith('.xml'):
                continue
            s = ET.parse(os.path.join(lib, fn)).getroot()
            out.append('')
            out.append('LIBRARY/%s: %s name=%r symbolType=%s' % (fn, tag(s), s.get('name'), s.get('symbolType', 'movie clip')))
            for tl in kids(find(s, 'timeline') or ET.Element('x'), 'DOMTimeline'):
                timeline(tl, '  ', out)
    print('\n'.join(out))


if __name__ == '__main__':
    sys.stdout.reconfigure(encoding='utf-8')
    main(sys.argv[1])
