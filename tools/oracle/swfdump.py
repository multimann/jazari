"""Minimal SWF dumper for the Al-Jazari fixtures (SWF 8 / AS2 subset).

usage: python swfdump.py file.swf [--header] [--shapes-full]

Prints the header (signature, version, stage size, frame rate, frame count)
and every tag, decoding what the fixtures use: shapes (fill/line styles incl.
LINESTYLE2 caps/joins, gradients incl. focal), PlaceObject2/3 (matrix,
cxform, name, clip actions, filters, blend mode, cacheAsBitmap), sprites,
buttons, text, fonts (with code tables), bitmaps, frame labels and a small
AVM1 disassembler.
"""
import struct
import sys
import zlib

TAGS = {0: 'End', 1: 'ShowFrame', 2: 'DefineShape', 4: 'PlaceObject', 5: 'RemoveObject', 6: 'DefineBits',
        7: 'DefineButton', 8: 'JPEGTables', 9: 'SetBackgroundColor', 10: 'DefineFont', 11: 'DefineText',
        12: 'DoAction', 13: 'DefineFontInfo', 14: 'DefineSound', 15: 'StartSound', 17: 'DefineButtonSound',
        18: 'SoundStreamHead', 19: 'SoundStreamBlock', 20: 'DefineBitsLossless', 21: 'DefineBitsJPEG2',
        22: 'DefineShape2', 23: 'DefineButtonCxform', 24: 'Protect', 26: 'PlaceObject2', 28: 'RemoveObject2',
        32: 'DefineShape3', 33: 'DefineText2', 34: 'DefineButton2', 35: 'DefineBitsJPEG3',
        36: 'DefineBitsLossless2', 37: 'DefineEditText', 39: 'DefineSprite', 43: 'FrameLabel',
        45: 'SoundStreamHead2', 46: 'DefineMorphShape', 48: 'DefineFont2', 56: 'ExportAssets',
        57: 'ImportAssets', 58: 'EnableDebugger', 59: 'DoInitAction', 60: 'DefineVideoStream', 61: 'VideoFrame',
        62: 'DefineFontInfo2', 64: 'EnableDebugger2', 65: 'ScriptLimits', 66: 'SetTabIndex',
        69: 'FileAttributes', 70: 'PlaceObject3', 71: 'ImportAssets2', 73: 'DefineFontAlignZones',
        74: 'CSMTextSettings', 75: 'DefineFont3', 77: 'Metadata', 78: 'DefineScalingGrid',
        83: 'DefineShape4', 84: 'DefineMorphShape2', 88: 'DefineFontName'}
BLEND = {0: 'normal', 1: 'normal', 2: 'layer', 3: 'multiply', 4: 'screen', 5: 'lighten', 6: 'darken',
         7: 'difference', 8: 'add', 9: 'subtract', 10: 'invert', 11: 'alpha', 12: 'erase', 13: 'overlay',
         14: 'hardlight'}
CAPS = {0: 'round', 1: 'none', 2: 'square'}
JOINS = {0: 'round', 1: 'bevel', 2: 'miter'}
SPREAD = {0: 'pad', 1: 'reflect', 2: 'repeat', 3: 'reserved'}
INTERP = {0: 'rgb', 1: 'linearRGB'}
CLIPEVENTS = ['keyUp', 'keyDown', 'mouseUp', 'mouseDown', 'mouseMove', 'unload', 'enterFrame', 'load',
              'dragOver', 'rollOut', 'rollOver', 'releaseOutside', 'release', 'press', 'initialize', 'data',
              None, None, None, None, None, 'construct', 'keyPress', 'dragOut']
BUTTONCOND = ['idleToOverDown', 'outDownToIdle', 'outDownToOverDown', 'overDownToOutDown',
              'overDownToOverUp', 'overUpToOverDown', 'overUpToIdle', 'idleToOverUp']
OPS = {0x04: 'NextFrame', 0x05: 'PrevFrame', 0x06: 'Play', 0x07: 'Stop', 0x08: 'ToggleQuality',
       0x09: 'StopSounds', 0x0A: 'Add', 0x0B: 'Subtract', 0x0C: 'Multiply', 0x0D: 'Divide', 0x0E: 'Equals',
       0x0F: 'Less', 0x10: 'And', 0x11: 'Or', 0x12: 'Not', 0x13: 'StringEquals', 0x14: 'StringLength',
       0x15: 'StringExtract', 0x17: 'Pop', 0x18: 'ToInteger', 0x1C: 'GetVariable', 0x1D: 'SetVariable',
       0x20: 'SetTarget2', 0x21: 'StringAdd', 0x22: 'GetProperty', 0x23: 'SetProperty',
       0x24: 'CloneSprite', 0x25: 'RemoveSprite', 0x26: 'Trace', 0x27: 'StartDrag', 0x28: 'EndDrag',
       0x29: 'StringLess', 0x2A: 'Throw', 0x2B: 'CastOp', 0x2C: 'ImplementsOp', 0x30: 'RandomNumber',
       0x31: 'MBStringLength', 0x32: 'CharToAscii', 0x33: 'AsciiToChar', 0x34: 'GetTime',
       0x35: 'MBStringExtract', 0x36: 'MBCharToAscii', 0x37: 'MBAsciiToChar', 0x3A: 'Delete',
       0x3B: 'Delete2', 0x3C: 'DefineLocal', 0x3D: 'CallFunction', 0x3E: 'Return', 0x3F: 'Modulo',
       0x40: 'NewObject', 0x41: 'DefineLocal2', 0x42: 'InitArray', 0x43: 'InitObject', 0x44: 'TypeOf',
       0x45: 'TargetPath', 0x46: 'Enumerate', 0x47: 'Add2', 0x48: 'Less2', 0x49: 'Equals2',
       0x4A: 'ToNumber', 0x4B: 'ToString', 0x4C: 'PushDuplicate', 0x4D: 'StackSwap', 0x4E: 'GetMember',
       0x4F: 'SetMember', 0x50: 'Increment', 0x51: 'Decrement', 0x52: 'CallMethod', 0x53: 'NewMethod',
       0x54: 'InstanceOf', 0x55: 'Enumerate2', 0x60: 'BitAnd', 0x61: 'BitOr', 0x62: 'BitXor',
       0x63: 'BitLShift', 0x64: 'BitRShift', 0x65: 'BitURShift', 0x66: 'StrictEquals', 0x67: 'Greater',
       0x68: 'StringGreater', 0x69: 'Extends', 0x81: 'GotoFrame', 0x83: 'GetURL', 0x87: 'StoreRegister',
       0x88: 'ConstantPool', 0x8A: 'WaitForFrame', 0x8B: 'SetTarget', 0x8C: 'GoToLabel',
       0x8D: 'WaitForFrame2', 0x8E: 'DefineFunction2', 0x8F: 'Try', 0x94: 'With', 0x96: 'Push',
       0x99: 'Jump', 0x9A: 'GetURL2', 0x9B: 'DefineFunction', 0x9D: 'If', 0x9E: 'Call',
       0x9F: 'GotoFrame2'}


class R:
    """Byte + bit reader over a bytes object."""

    def __init__(self, d, pos=0, end=None):
        self.d, self.p, self.bitpos, self.end = d, pos, 0, len(d) if end is None else end

    def align(self):
        if self.bitpos:
            self.bitpos = 0
            self.p += 1

    def ub(self, n):
        v = 0
        for _ in range(n):
            v = (v << 1) | ((self.d[self.p] >> (7 - self.bitpos)) & 1)
            self.bitpos += 1
            if self.bitpos == 8:
                self.bitpos = 0
                self.p += 1
        return v

    def sb(self, n):
        if n == 0:
            return 0
        v = self.ub(n)
        return v - (1 << n) if v & (1 << (n - 1)) else v

    def fb(self, n):
        return self.sb(n) / 65536.0

    def u8(self):
        self.align(); v = self.d[self.p]; self.p += 1; return v

    def u16(self):
        self.align(); v = struct.unpack_from('<H', self.d, self.p)[0]; self.p += 2; return v

    def s16(self):
        self.align(); v = struct.unpack_from('<h', self.d, self.p)[0]; self.p += 2; return v

    def u32(self):
        self.align(); v = struct.unpack_from('<I', self.d, self.p)[0]; self.p += 4; return v

    def fixed(self):
        self.align(); v = struct.unpack_from('<i', self.d, self.p)[0] / 65536.0; self.p += 4; return v

    def fixed8(self):
        self.align(); v = struct.unpack_from('<h', self.d, self.p)[0] / 256.0; self.p += 2; return v

    def f32(self):
        self.align(); v = struct.unpack_from('<f', self.d, self.p)[0]; self.p += 4; return v

    def f64(self):  # AVM1 double: two little-endian 32-bit halves, high word first
        self.align()
        hi, lo = self.d[self.p:self.p + 4], self.d[self.p + 4:self.p + 8]
        self.p += 8
        return struct.unpack('<d', lo + hi)[0]

    def bytes(self, n):
        self.align(); v = self.d[self.p:self.p + n]; self.p += n; return v

    def string(self):
        self.align()
        e = self.d.index(0, self.p)
        raw = self.d[self.p:e]
        self.p = e + 1
        try:
            return raw.decode('utf-8')
        except UnicodeDecodeError:
            return raw.decode('latin-1')

    def rgb(self):
        r, g, b = self.u8(), self.u8(), self.u8()
        return '#%02X%02X%02X' % (r, g, b)

    def rgba(self):
        r, g, b, a = self.u8(), self.u8(), self.u8(), self.u8()
        return '#%02X%02X%02X/a%d' % (r, g, b, a)

    def rect(self):
        self.align()
        n = self.ub(5)
        v = [self.sb(n) for _ in range(4)]
        self.align()
        return v  # xmin xmax ymin ymax in twips

    def matrix(self):
        self.align()
        a = d = 1.0
        b = c = 0.0
        if self.ub(1):
            n = self.ub(5); a = self.fb(n); d = self.fb(n)
        if self.ub(1):
            n = self.ub(5); b = self.fb(n); c = self.fb(n)
        n = self.ub(5)
        tx, ty = self.sb(n), self.sb(n)
        self.align()
        return 'matrix(a=%.4f b=%.4f c=%.4f d=%.4f tx=%.2f ty=%.2f)' % (a, b, c, d, tx / 20.0, ty / 20.0)

    def cxform(self, alpha):
        self.align()
        has_add, has_mult, n = self.ub(1), self.ub(1), self.ub(4)
        k = 4 if alpha else 3
        mult = [self.sb(n) for _ in range(k)] if has_mult else None
        add = [self.sb(n) for _ in range(k)] if has_add else None
        self.align()
        s = 'cxform('
        if mult: s += 'mult=' + ','.join('%.3f' % (m / 256.0) for m in mult) + ' '
        if add: s += 'add=' + ','.join(str(x) for x in add)
        return s.strip() + ')'


def rect_px(r):
    return '(%g,%g)-(%g,%g)px' % (r[0] / 20.0, r[2] / 20.0, r[1] / 20.0, r[3] / 20.0)


# ---------------------------------------------------------------- shapes
def fillstyle(r, ver):
    t = r.u8()
    if t == 0x00:
        return 'solid ' + (r.rgba() if ver >= 3 else r.rgb())
    if t in (0x10, 0x12, 0x13):
        m = r.matrix()
        spread, interp, n = r.ub(2), r.ub(2), r.ub(4)
        stops = []
        for _ in range(n):
            ratio = r.u8()
            stops.append('%d:%s' % (ratio, r.rgba() if ver >= 3 else r.rgb()))
        kind = {0x10: 'linear', 0x12: 'radial', 0x13: 'focal'}[t]
        s = '%s spread=%s interp=%s %s stops=[%s]' % (kind, SPREAD[spread], INTERP.get(interp, interp), m,
                                                      ' '.join(stops))
        if t == 0x13:
            s += ' focalPoint=%.4f' % r.fixed8()
        return s
    if t in (0x40, 0x41, 0x42, 0x43):
        cid = r.u16()
        m = r.matrix()
        kind = {0x40: 'repeating', 0x41: 'clipped', 0x42: 'repeating-nonsmoothed',
                0x43: 'clipped-nonsmoothed'}[t]
        return 'bitmap %s id=%d %s' % (kind, cid, m)
    raise ValueError('unknown fill type 0x%02X' % t)


def styles(r, ver, out, indent):
    n = r.u8()
    if n == 0xFF and ver >= 2:
        n = r.u16()
    for i in range(n):
        out.append('%sfill %d: %s' % (indent, i + 1, fillstyle(r, ver)))
    n = r.u8()
    if n == 0xFF and ver >= 2:
        n = r.u16()
    for i in range(n):
        w = r.u16()
        if ver == 4:
            sc, jn, hasfill, nohs, novs, ph = r.ub(2), r.ub(2), r.ub(1), r.ub(1), r.ub(1), r.ub(1)
            r.ub(5)
            noclose, ec = r.ub(1), r.ub(2)
            s = 'width=%gpx startCap=%s endCap=%s join=%s' % (w / 20.0, CAPS[sc], CAPS[ec], JOINS[jn])
            if jn == 2:
                s += ' miterLimit=%g' % r.fixed8()
            scale = 'normal' if not (nohs or novs) else ('none' if nohs and novs else
                                                         ('vertical' if nohs else 'horizontal'))
            s += ' scale=%s pixelHinting=%d noClose=%d' % (scale, ph, noclose)
            s += ' fill=' + (fillstyle(r, ver) if hasfill else r.rgba())
        else:
            s = 'width=%gpx color=%s' % (w / 20.0, r.rgba() if ver >= 3 else r.rgb())
        out.append('%sline %d: %s' % (indent, i + 1, s))
    r.align()
    return r.ub(4), r.ub(4)


def shape(r, ver, out, indent, full):
    nfb, nlb = styles(r, ver, out, indent)
    straight = curved = moves = newstyles = 0
    fs0 = fs1 = ls = 0
    usage = {}  # (style block, 'fill'|'line', index) -> edge count
    bounds = {}  # same key -> [xmin, ymin, xmax, ymax] in twips (anchor and control points)
    x = y = 0

    def use(pts):
        for kind, idx in (('fill', fs0), ('fill', fs1), ('line', ls)):
            if idx:
                key = (newstyles, kind, idx)
                usage[key] = usage.get(key, 0) + 1
                b = bounds.setdefault(key, [pts[0][0], pts[0][1], pts[0][0], pts[0][1]])
                for px, py in pts:
                    b[0] = min(b[0], px); b[1] = min(b[1], py); b[2] = max(b[2], px); b[3] = max(b[3], py)
    while True:
        if r.ub(1) == 0:
            flags = r.ub(5)
            if flags == 0:
                break
            if flags & 1:
                n = r.ub(5); x = r.sb(n); y = r.sb(n); moves += 1
            if flags & 2: fs0 = r.ub(nfb)
            if flags & 4: fs1 = r.ub(nfb)
            if flags & 8: ls = r.ub(nlb)
            if flags & 16:
                newstyles += 1
                fs0 = fs1 = ls = 0
                out.append('%s-- new styles (block %d) --' % (indent, newstyles))
                nfb, nlb = styles(r, ver, out, indent)
        else:
            x0, y0 = x, y
            if r.ub(1):
                n = r.ub(4) + 2
                if r.ub(1):
                    x += r.sb(n); y += r.sb(n)
                elif r.ub(1):
                    y += r.sb(n)
                else:
                    x += r.sb(n)
                straight += 1
                use([(x0, y0), (x, y)])
            else:
                n = r.ub(4) + 2
                cx = x + r.sb(n); cy = y + r.sb(n)
                x = cx + r.sb(n); y = cy + r.sb(n)
                curved += 1
                use([(x0, y0), (cx, cy), (x, y)])
    out.append('%sedges: %d straight, %d curved, %d moveTo, %d new-style blocks' %
               (indent, straight, curved, moves, newstyles))
    out.append('%sstyle usage (block:kind index=edges): %s' % (indent, ' '.join(
        '%d:%s%d=%d' % (b, k, i, n) for (b, k, i), n in sorted(usage.items()))))
    if full:
        # Edge bounds per style, in pixels, in the shape's own coordinate space.
        for (b, k, i), (x0, y0, x1, y1) in sorted(bounds.items()):
            out.append('%s  bounds %d:%s%d = (%g,%g)-(%g,%g)' % (indent, b, k, i, x0 / 20.0, y0 / 20.0, x1 / 20.0, y1 / 20.0))


# ---------------------------------------------------------------- filters
def filters(r):
    out = []
    for _ in range(r.u8()):
        fid = r.u8()
        if fid == 0:
            c = r.rgba(); bx, by, ang, dist, st = r.fixed(), r.fixed(), r.fixed(), r.fixed(), r.fixed8()
            inner, ko, comp, passes = r.ub(1), r.ub(1), r.ub(1), r.ub(5)
            out.append('DropShadow(color=%s blur=%g,%g angle=%.4frad distance=%g strength=%g inner=%d '
                       'knockout=%d composite=%d passes=%d)' % (c, bx, by, ang, dist, st, inner, ko, comp, passes))
        elif fid == 1:
            bx, by = r.fixed(), r.fixed(); passes = r.ub(5); r.ub(3)
            out.append('Blur(blur=%g,%g passes=%d)' % (bx, by, passes))
        elif fid == 2:
            c = r.rgba(); bx, by, st = r.fixed(), r.fixed(), r.fixed8()
            inner, ko, comp, passes = r.ub(1), r.ub(1), r.ub(1), r.ub(5)
            out.append('Glow(color=%s blur=%g,%g strength=%g inner=%d knockout=%d composite=%d passes=%d)' %
                       (c, bx, by, st, inner, ko, comp, passes))
        elif fid == 3:
            # the SWF spec lists ShadowColor first, but Flash writes HighlightColor first
            hc, sc = r.rgba(), r.rgba()
            bx, by, ang, dist, st = r.fixed(), r.fixed(), r.fixed(), r.fixed(), r.fixed8()
            inner, ko, comp, ontop, passes = r.ub(1), r.ub(1), r.ub(1), r.ub(1), r.ub(4)
            out.append('Bevel(shadow=%s highlight=%s blur=%g,%g angle=%.4frad distance=%g strength=%g '
                       'inner=%d knockout=%d composite=%d onTop=%d passes=%d)' %
                       (sc, hc, bx, by, ang, dist, st, inner, ko, comp, ontop, passes))
        elif fid in (4, 7):
            n = r.u8(); cols = [r.rgba() for _ in range(n)]; rat = [r.u8() for _ in range(n)]
            bx, by, ang, dist, st = r.fixed(), r.fixed(), r.fixed(), r.fixed(), r.fixed8()
            inner, ko, comp, ontop, passes = r.ub(1), r.ub(1), r.ub(1), r.ub(1), r.ub(4)
            out.append('%s(colors=%s ratios=%s blur=%g,%g angle=%.4f distance=%g strength=%g inner=%d '
                       'knockout=%d onTop=%d passes=%d)' % ('GradientGlow' if fid == 4 else 'GradientBevel',
                                                          cols, rat, bx, by, ang, dist, st, inner, ko, ontop, passes))
        elif fid == 5:
            mx, my = r.u8(), r.u8(); div, bias = r.f32(), r.f32()
            m = [r.f32() for _ in range(mx * my)]; c = r.rgba(); r.ub(6); clamp, pa = r.ub(1), r.ub(1)
            out.append('Convolution(%dx%d div=%g bias=%g m=%s color=%s clamp=%d preserveAlpha=%d)' %
                       (mx, my, div, bias, m, c, clamp, pa))
        elif fid == 6:
            m = [r.f32() for _ in range(20)]
            out.append('ColorMatrix(%s)' % ' '.join('%.4g' % x for x in m))
        else:
            out.append('unknown filter %d' % fid)
            break
    return out


# ---------------------------------------------------------------- AVM1
def avm1(d, indent, out, ver):
    r = R(d)
    while r.p < len(d):
        off = r.p
        code = r.u8()
        if code == 0:
            out.append('%s%04d End' % (indent, off))
            break
        name = OPS.get(code, 'op0x%02X' % code)
        if code < 0x80:
            out.append('%s%04d %s' % (indent, off, name))
            continue
        ln = r.u16()
        body = R(d, r.p, r.p + ln)
        arg = ''
        try:
            if code == 0x88:
                arg = repr([body.string() for _ in range(body.u16())])
            elif code == 0x96:
                vals = []
                while body.p < r.p + ln:
                    t = body.u8()
                    if t == 0: vals.append(repr(body.string()))
                    elif t == 1: vals.append('%gf' % body.f32())
                    elif t == 2: vals.append('null')
                    elif t == 3: vals.append('undefined')
                    elif t == 4: vals.append('r%d' % body.u8())
                    elif t == 5: vals.append('true' if body.u8() else 'false')
                    elif t == 6: vals.append('%r' % body.f64())
                    elif t == 7: vals.append('%d' % struct.unpack('<i', body.bytes(4))[0])
                    elif t == 8: vals.append('c%d' % body.u8())
                    elif t == 9: vals.append('c%d' % body.u16())
                    else: vals.append('?%d' % t); break
                arg = ' '.join(vals)
            elif code in (0x8C,):
                arg = repr(body.string())
            elif code == 0x81:
                arg = 'frame %d' % body.u16()
            elif code in (0x99, 0x9D):
                arg = 'offset %d' % body.s16()
            elif code == 0x87:
                arg = 'r%d' % body.u8()
            elif code == 0x9B:
                fn = body.string(); n = body.u16(); ps = [body.string() for _ in range(n)]; size = body.u16()
                arg = '%s(%s) body=%d bytes' % (fn or '<anon>', ','.join(ps), size)
            elif code == 0x8E:
                fn = body.string(); n = body.u16(); regs = body.u8(); fl = body.u16()
                ps = []
                for _ in range(n):
                    reg = body.u8(); ps.append('%s@r%d' % (body.string(), reg))
                size = body.u16()
                arg = '%s(%s) regs=%d flags=0x%04X body=%d bytes' % (fn or '<anon>', ','.join(ps), regs, fl, size)
            elif code == 0x83:
                arg = '%r %r' % (body.string(), body.string())
            elif code == 0x9F:
                f = body.u8(); arg = 'play=%d' % (f & 1)
                if f & 2:
                    arg += ' sceneBias=%d' % body.u16()
            else:
                arg = body.bytes(ln).hex()
        except Exception as e:  # keep going on odd payloads
            arg = '(decode error %s)' % e
        out.append('%s%04d %s %s' % (indent, off, name, arg))
        r.p += ln


# ---------------------------------------------------------------- tags
def tags(d, pos, end, out, indent, ver, full):
    frame = 1
    while pos < end:
        code_len = struct.unpack_from('<H', d, pos)[0]
        code, ln = code_len >> 6, code_len & 0x3F
        pos += 2
        if ln == 0x3F:
            ln = struct.unpack_from('<I', d, pos)[0]; pos += 4
        body = d[pos:pos + ln]
        name = TAGS.get(code, 'Tag%d' % code)
        r = R(body)
        line = '%s[%d] %s (%d bytes)' % (indent, code, name, ln)
        sub = []
        try:
            if code == 1:
                line = '%s---- ShowFrame (end of frame %d)' % (indent, frame); frame += 1
            elif code == 9:
                line += ' ' + r.rgb()
            elif code == 69:
                f = r.u32(); line += ' flags=0x%X' % f
            elif code in (2, 22, 32, 83):
                v = {2: 1, 22: 2, 32: 3, 83: 4}[code]
                cid = r.u16(); b = r.rect()
                line += ' id=%d bounds=%s' % (cid, rect_px(b))
                if v == 4:
                    eb = r.rect(); fl = r.u8()
                    line += ' edgeBounds=%s nonScaling=%d scaling=%d windingRule=%d' % (
                        rect_px(eb), (fl >> 1) & 1, fl & 1, (fl >> 2) & 1)
                shape(r, v, sub, indent + '    ', full)
            elif code in (26, 70):
                f1 = r.u8(); f2 = r.u8() if code == 70 else 0
                depth = r.u16()
                parts = ['depth=%d' % depth]
                if f1 & 1: parts.append('move')
                if code == 70 and (f2 & 8 or (f2 & 16 and f1 & 2)): parts.append('class=' + r.string())
                if f1 & 2: parts.append('char=%d' % r.u16())
                if f1 & 4: parts.append(r.matrix())
                if f1 & 8: parts.append(r.cxform(True))
                if f1 & 16: parts.append('ratio=%d' % r.u16())
                if f1 & 32: parts.append('name=%r' % r.string())
                if f1 & 64: parts.append('clipDepth=%d' % r.u16())
                if f2 & 1: sub += ['%s    filter: %s' % (indent, x) for x in filters(r)]
                if f2 & 2: parts.append('blend=%s' % BLEND.get(r.u8(), '?'))
                if f2 & 4: parts.append('cacheAsBitmap=%d' % r.u8())
                if f1 & 128:
                    r.u16(); r.u32() if ver >= 6 else r.u16()
                    while True:
                        ev = r.u32() if ver >= 6 else r.u16()
                        if ev == 0: break
                        size = r.u32()
                        # flag bytes are stored in order; bit 7 of the first byte is keyUp
                        fb = [(ev >> (8 * k)) & 0xFF for k in range(4)]
                        names = [CLIPEVENTS[i] for i in range(24)
                                 if CLIPEVENTS[i] and (fb[i // 8] >> (7 - i % 8)) & 1]
                        key = ''
                        start = r.p
                        if 'keyPress' in names: key = ' key=%d' % r.u8()
                        sub.append('%s    onClipEvent(%s)%s:' % (indent, ','.join(names), key))
                        avm1(body[r.p:start + size], indent + '      ', sub, ver)
                        r.p = start + size
                line += ' ' + ' '.join(parts)
            elif code == 28:
                line += ' depth=%d' % r.u16()
            elif code == 39:
                cid, fc = r.u16(), r.u16()
                line += ' id=%d frameCount=%d' % (cid, fc)
                tags(body, 4, len(body), sub, indent + '    ', ver, full)
            elif code == 43:
                line += ' %r' % r.string()
                if r.p < len(body): line += ' namedAnchor=%d' % r.u8()
            elif code == 12:
                avm1(body, indent + '    ', sub, ver)
            elif code == 59:
                line += ' sprite=%d' % r.u16(); avm1(body[2:], indent + '    ', sub, ver)
            elif code == 34:
                cid = r.u16(); fl = r.u8(); aoff = r.u16()
                line += ' id=%d trackAsMenu=%d' % (cid, fl & 1)
                while True:
                    bf = r.u8()
                    if bf == 0: break
                    states = [n for n, bit in (('up', 1), ('over', 2), ('down', 4), ('hit', 8)) if bf & bit]
                    ch, dp = r.u16(), r.u16(); m = r.matrix(); cx = r.cxform(True)
                    extra = ''
                    if bf & 16: extra += ' filters=%s' % filters(r)
                    if bf & 32: extra += ' blend=%s' % BLEND.get(r.u8())
                    sub.append('%s    record states=%s char=%d depth=%d %s %s%s' % (indent, '+'.join(states), ch, dp, m, cx, extra))
                if aoff:
                    p = 3 + aoff
                    while True:
                        size = struct.unpack_from('<H', body, p)[0]
                        cond = struct.unpack_from('<H', body, p + 2)[0]
                        b0, b1 = cond & 0xFF, cond >> 8
                        names = [BUTTONCOND[i] for i in range(8) if (b0 >> (7 - i)) & 1]
                        if b1 & 1: names.append('overDownToIdle')
                        key = b1 >> 1
                        sub.append('%s    condition %s%s:' % (indent, ','.join(names), ' key=%d' % key if key else ''))
                        avm1(body[p + 4:(p + size) if size else len(body)], indent + '      ', sub, ver)
                        if size == 0: break
                        p += size
            elif code == 37:
                cid = r.u16(); b = r.rect()
                f1, f2 = r.u8(), r.u8()
                fl = {'hasText': f1 & 128, 'wordWrap': f1 & 64, 'multiline': f1 & 32, 'password': f1 & 16,
                      'readOnly': f1 & 8, 'hasColor': f1 & 4, 'hasMaxLen': f1 & 2, 'hasFont': f1 & 1,
                      'hasFontClass': f2 & 128, 'autoSize': f2 & 64, 'hasLayout': f2 & 32, 'noSelect': f2 & 16,
                      'border': f2 & 8, 'wasStatic': f2 & 4, 'html': f2 & 2, 'useOutlines': f2 & 1}
                line += ' id=%d bounds=%s flags=%s' % (cid, rect_px(b), ','.join(k for k, v in fl.items() if v))
                if fl['hasFont']: line += ' font=%d' % r.u16()
                if fl['hasFontClass']: line += ' fontClass=%r' % r.string()
                if fl['hasFont']: line += ' height=%gpx' % (r.u16() / 20.0)
                if fl['hasColor']: line += ' color=%s' % r.rgba()
                if fl['hasMaxLen']: line += ' maxLength=%d' % r.u16()
                if fl['hasLayout']:
                    line += ' align=%s margins=%d,%d indent=%d leading=%d' % (
                        {0: 'left', 1: 'right', 2: 'center', 3: 'justify'}.get(r.u8()), r.u16(), r.u16(), r.u16(), r.s16())
                line += ' var=%r' % r.string()
                if fl['hasText']: line += ' text=%r' % r.string()
            elif code in (11, 33):
                cid = r.u16(); b = r.rect(); m = r.matrix(); gb, ab = r.u8(), r.u8()
                line += ' id=%d bounds=%s %s' % (cid, rect_px(b), m)
                while True:
                    r.align()
                    first = r.ub(1)
                    if first == 0:
                        break
                    r.ub(3); hf, hc, hy, hx = r.ub(1), r.ub(1), r.ub(1), r.ub(1)
                    rec = []
                    if hf: rec.append('font=%d' % r.u16())
                    if hc: rec.append('color=%s' % (r.rgba() if code == 33 else r.rgb()))
                    if hx: rec.append('x=%g' % (r.s16() / 20.0))
                    if hy: rec.append('y=%g' % (r.s16() / 20.0))
                    if hf: rec.append('height=%gpx' % (r.u16() / 20.0))
                    n = r.u8()
                    glyphs = []
                    for _ in range(n):
                        gi = r.ub(gb); adv = r.sb(ab); glyphs.append(gi)
                    rec.append('glyphs=%s' % glyphs)
                    sub.append('%s    record %s' % (indent, ' '.join(rec)))
            elif code in (48, 75):
                cid = r.u16(); fl = r.u8(); lang = r.u8(); nl = r.u8()
                fname = r.bytes(nl).rstrip(b'\0').decode('latin-1')
                ng = r.u16()
                wide_off, wide_codes = fl & 8, fl & 4
                line += ' id=%d name=%r glyphs=%d bold=%d italic=%d layout=%d lang=%d' % (
                    cid, fname, ng, fl & 1, (fl >> 1) & 1, (fl >> 7) & 1, lang)
                if ng:
                    base = r.p
                    offs = [struct.unpack_from('<I' if wide_off else '<H', body, base + i * (4 if wide_off else 2))[0]
                            for i in range(ng + 1)]
                    ct = base + offs[ng]
                    codes = [struct.unpack_from('<H', body, ct + 2 * i)[0] if wide_codes else body[ct + i]
                             for i in range(ng)]
                    sub.append('%s    codes: %s' % (indent, ' '.join('U+%04X' % c for c in codes)))
                    sub.append('%s    chars: %r' % (indent, ''.join(chr(c) for c in codes)))
            elif code == 88:
                line += ' font=%d name=%r copyright=%r' % (r.u16(), r.string(), r.string())
            elif code == 13 or code == 62:
                cid = r.u16(); nl = r.u8(); line += ' font=%d name=%r' % (cid, r.bytes(nl).decode('latin-1'))
            elif code in (20, 36):
                cid, fmt, w, h = r.u16(), r.u8(), r.u16(), r.u16()
                line += ' id=%d format=%d size=%dx%d' % (cid, fmt, w, h)
            elif code == 21:
                line += ' id=%d jpegBytes=%d' % (r.u16(), ln - 2)
            elif code == 35:
                cid = r.u16(); ao = r.u32()
                line += ' id=%d jpegBytes=%d alphaBytes=%d' % (cid, ao, ln - 6 - ao)
            elif code == 56:
                n = r.u16(); line += ' ' + ', '.join('%d=%r' % (r.u16(), r.string()) for _ in range(n))
            elif code == 65:
                line += ' maxRecursion=%d timeout=%ds' % (r.u16(), r.u16())
            elif code == 74:
                tid = r.u16(); ft, gf = r.ub(2), r.ub(3); r.ub(3)
                line += ' text=%d useFlashType=%d gridFit=%d thickness=%g sharpness=%g' % (
                    tid, ft, gf, r.f32(), r.f32())
            elif code == 73:
                line += ' font=%d' % r.u16()
            elif code == 78:
                line += ' char=%d splitter=%s' % (r.u16(), rect_px(r.rect()))
            elif code == 77:
                line += ' ' + body.decode('utf-8', 'replace')[:200]
        except Exception as e:
            line += '  !! decode error: %r' % e
        out.append(line)
        out.extend(sub)
        pos += ln
        if code == 0:
            break


def header(path):
    d = open(path, 'rb').read()
    sig = d[:3].decode('latin-1')
    ver = d[3]
    length = struct.unpack_from('<I', d, 4)[0]
    if sig == 'CWS':
        body = d[:8] + zlib.decompress(d[8:])
    elif sig == 'FWS':
        body = d
    else:
        raise ValueError('not a SWF: %r' % sig)
    r = R(body, 8)
    rc = r.rect()
    rate = r.u16() / 256.0
    frames = r.u16()
    return dict(sig=sig, version=ver, length=length, filesize=len(d), body=body, stage=(rc[1] - rc[0]) / 20.0,
                stage_h=(rc[3] - rc[2]) / 20.0, rate=rate, frames=frames, tags_at=r.p)


def main():
    path = sys.argv[1]
    h = header(path)
    print('%s: signature=%s version=%d declaredLength=%d fileSize=%d stage=%gx%g frameRate=%g frameCount=%d' % (
        path, h['sig'], h['version'], h['length'], h['filesize'], h['stage'], h['stage_h'], h['rate'], h['frames']))
    if '--header' in sys.argv:
        return
    out = []
    tags(h['body'], h['tags_at'], len(h['body']), out, '', h['version'], '--shapes-full' in sys.argv)
    print('\n'.join(out))


if __name__ == '__main__':
    sys.stdout.reconfigure(encoding='utf-8')
    main()
