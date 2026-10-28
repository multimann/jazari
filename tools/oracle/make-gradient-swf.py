"""Write a tiny SWF 8 with gradient-filled rectangles.

Flash 8's JSFL setCustomFill cannot create gradients from scratch (it stores
uninitialised memory), so the gradients fixture imports this SWF and reads
valid gradient Fill objects back from the imported shapes.

usage: python make-gradient-swf.py out.swf
Shapes (all 100x60 px, left to right at y=20):
  id1 DefineShape3 linear  red -> blue
  id2 DefineShape3 radial  white -> black
(Flash 8's SWF import silently drops DefineShape4, so no focal shape here;
focal gradients are made by setting focalPoint on the radial Fill object.)
"""
import struct
import sys


class W:
    def __init__(self):
        self.out = bytearray()
        self.acc = 0
        self.n = 0

    def ub(self, v, n):
        for i in range(n - 1, -1, -1):
            self.acc = (self.acc << 1) | ((v >> i) & 1)
            self.n += 1
            if self.n == 8:
                self.out.append(self.acc)
                self.acc = self.n = 0

    def sb(self, v, n):
        self.ub(v & ((1 << n) - 1), n)

    def align(self):
        if self.n:
            self.ub(0, 8 - self.n)

    def u8(self, v): self.align(); self.out.append(v)
    def u16(self, v): self.align(); self.out += struct.pack('<H', v)
    def raw(self, b): self.align(); self.out += b


def nbits_s(*vals):
    n = 1
    for v in vals:
        while not (-(1 << (n - 1)) <= v < (1 << (n - 1))):
            n += 1
    return n


def rect(w, x0, x1, y0, y1):
    n = nbits_s(x0, x1, y0, y1)
    w.align(); w.ub(n, 5)
    for v in (x0, x1, y0, y1):
        w.sb(v, n)
    w.align()


def matrix(w, a, d, tx, ty):
    w.align()
    w.ub(1, 1)
    fa, fd = int(round(a * 65536)), int(round(d * 65536))
    n = nbits_s(fa, fd); w.ub(n, 5); w.sb(fa, n); w.sb(fd, n)
    w.ub(0, 1)
    n = nbits_s(tx, ty); w.ub(n, 5); w.sb(tx, n); w.sb(ty, n)
    w.align()


def tag(code, body):
    if len(body) < 0x3F:
        return struct.pack('<H', (code << 6) | len(body)) + body
    return struct.pack('<HI', (code << 6) | 0x3F, len(body)) + body


def shape(cid, ver, kind, stops, x, y, wpx=100, hpx=60, focal=None):
    w = W()
    w.u16(cid)
    X0, Y0, X1, Y1 = x * 20, y * 20, (x + wpx) * 20, (y + hpx) * 20
    rect(w, X0, X1, Y0, Y1)
    if ver == 4:
        rect(w, X0, X1, Y0, Y1)
        w.u8(0)
    w.u8(1)  # one fill style
    w.u8({'linear': 0x10, 'radial': 0x12, 'focal': 0x13}[kind])
    matrix(w, wpx * 20 / 32768.0, hpx * 20 / 32768.0, (X0 + X1) // 2, (Y0 + Y1) // 2)
    w.ub(0, 2); w.ub(0, 2); w.ub(len(stops), 4)
    for ratio, (r, g, b, a) in stops:
        w.u8(ratio); w.u8(r); w.u8(g); w.u8(b); w.u8(a)
    if focal is not None:
        w.raw(struct.pack('<h', int(focal * 256)))
    w.u8(0)  # no line styles
    w.align(); w.ub(1, 4); w.ub(0, 4)  # numFillBits=1, numLineBits=0
    # style change: moveTo + fillStyle1
    w.ub(0, 1); w.ub(0b00101, 5)
    n = nbits_s(X0, Y0); w.ub(n, 5); w.sb(X0, n); w.sb(Y0, n)
    w.ub(1, 1)  # fillStyle1 = 1
    for dx, dy in ((X1 - X0, 0), (0, Y1 - Y0), (X0 - X1, 0), (0, Y0 - Y1)):
        n = nbits_s(dx, dy)
        w.ub(1, 1); w.ub(1, 1); w.ub(n - 2, 4)
        if dx == 0:
            w.ub(0, 1); w.ub(1, 1); w.sb(dy, n)
        else:
            w.ub(0, 1); w.ub(0, 1); w.sb(dx, n)
    w.ub(0, 6)  # end record
    w.align()
    return tag(32 if ver == 3 else 83, bytes(w.out))


def place(cid, depth):
    w = W()
    w.u8(0x06); w.u16(depth); w.u16(cid)
    matrix(w, 1.0, 1.0, 0, 0)
    return tag(26, bytes(w.out))


def main(path):
    body = W()
    rect(body, 0, 550 * 20, 0, 400 * 20)
    body.u16(12 << 8)
    body.u16(1)
    tags = tag(69, struct.pack('<I', 0)) + tag(9, b'\xff\xff\xff')
    tags += shape(1, 3, 'linear', [(0, (255, 0, 0, 255)), (255, (0, 0, 255, 255))], 20, 20)
    tags += shape(2, 3, 'radial', [(0, (255, 255, 255, 255)), (255, (0, 0, 0, 255))], 140, 20)
    tags += place(1, 1) + place(2, 2)
    tags += tag(1, b'') + tag(0, b'')
    data = bytes(body.out) + tags
    swf = b'FWS' + bytes([8]) + struct.pack('<I', 8 + len(data)) + data
    open(path, 'wb').write(swf)
    print('wrote', path, len(swf), 'bytes')


if __name__ == '__main__':
    main(sys.argv[1])
