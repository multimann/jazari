"""Check every fixture and print a Markdown report (used for tests/fixtures/MANIFEST.md).

usage: python verify-fixtures.py [--root DIR] [--tracecap DIR] [NAME ...]

--root defaults to tests/fixtures of this repository and --tracecap to
target/oracle/tracecap (where capture-trace.ps1 leaves the Test Movie SWFs).
Without names, every folder of --root that holds NAME/NAME.fla is checked.

For each fixture: FLA/SWF/XFL presence and sizes; the SWF header (first bytes:
signature, version, declared length) plus stage size, frame rate and frame
count from the (decompressed) header; the tag census; the JPEXS
conversion-report lines other than the routine PUBLISH_PROPERTY/EXPORT_SCOPE
notes; the Test Movie trace (flash8-trace.txt); and, when the Test Movie SWF
is still in --tracecap, whether it is byte-identical to flash8.swf after zlib
decompression.
"""
import collections
import os
import struct
import sys

sys.dont_write_bytecode = True  # no __pycache__ in tools/oracle
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import swfdump  # noqa: E402

REPO = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
ORDER = ['shapes-basic', 'gradients', 'symbols', 'tweens', 'as2', 'text', 'bitmaps', 'smoke']


def census(body, pos, end, counts):
    while pos < end:
        cl = struct.unpack_from('<H', body, pos)[0]
        code, ln = cl >> 6, cl & 0x3F
        pos += 2
        if ln == 0x3F:
            ln = struct.unpack_from('<I', body, pos)[0]
            pos += 4
        counts[swfdump.TAGS.get(code, 'Tag%d' % code)] += 1
        if code == 39:
            census(body, pos + 4, pos + ln, counts)
        pos += ln
        if code == 0:
            break


def discover(root):
    found = [n for n in sorted(os.listdir(root))
             if not n.startswith('_') and os.path.isfile(os.path.join(root, n, n + '.fla'))]
    return [n for n in ORDER if n in found] + [n for n in found if n not in ORDER]


def same_movie(a, b):
    """True when two SWFs are identical after zlib decompression (header length included)."""
    try:
        return swfdump.header(a)['body'] == swfdump.header(b)['body']
    except (OSError, ValueError):
        return None


def main(argv):
    sys.stdout.reconfigure(encoding='utf-8')
    root = os.path.join(REPO, 'tests', 'fixtures')
    tracecap = os.path.join(REPO, 'target', 'oracle', 'tracecap')
    names = []
    it = iter(argv)
    for a in it:
        if a == '--root':
            root = next(it)
        elif a == '--tracecap':
            tracecap = next(it)
        else:
            names.append(a)
    names = names or discover(root)
    print('| fixture | FLA bytes | SWF bytes | first 8 bytes | sig | ver | stage | fps | frames | xfl/DOMDocument.xml | result |')
    print('|---|---|---|---|---|---|---|---|---|---|---|')
    details = []
    bad = 0
    for n in names:
        d = os.path.join(root, n)
        fla, swf, dom = os.path.join(d, n + '.fla'), os.path.join(d, 'flash8.swf'), os.path.join(d, 'xfl', 'DOMDocument.xml')
        if not os.path.isfile(swf):
            print('| %s | %d | MISSING | | | | | | | %s | FAIL |' % (
                n, os.path.getsize(fla) if os.path.isfile(fla) else -1, 'yes' if os.path.isfile(dom) else 'MISSING'))
            bad += 1
            continue
        ok = os.path.isfile(fla) and os.path.isfile(dom)
        first = open(swf, 'rb').read(8)
        h = swfdump.header(swf)
        good = ok and h['version'] == 8 and h['sig'] in ('CWS', 'FWS')
        bad += 0 if good else 1
        print('| %s | %d | %d | `%s` | %s | %d | %gx%g | %g | %d | %s | %s |' % (
            n, os.path.getsize(fla) if os.path.isfile(fla) else -1, os.path.getsize(swf),
            first.hex(' '), h['sig'], h['version'], h['stage'], h['stage_h'], h['rate'], h['frames'],
            'yes (%d B)' % os.path.getsize(dom) if os.path.isfile(dom) else 'MISSING', 'OK' if good else 'FAIL'))
        counts = collections.Counter()
        census(h['body'], h['tags_at'], len(h['body']), counts)
        rep = os.path.join(d, 'xfl', 'conversion-report.txt')
        notes = []
        if os.path.isfile(rep):
            for line in open(rep, encoding='utf-8', errors='replace'):
                line = line.strip()
                if ':' in line and not line.startswith(('PUBLISH_PROPERTY', 'EXPORT_SCOPE')) and '[' in line:
                    notes.append(line)
        else:
            notes.append('(no conversion-report.txt)')
        tr = os.path.join(d, 'flash8-trace.txt')
        trace = open(tr, encoding='utf-8-sig', errors='replace').read().splitlines() if os.path.isfile(tr) else None
        test_swf = os.path.join(tracecap, n, n + '.swf')
        identical = same_movie(test_swf, swf) if os.path.isfile(test_swf) else None
        details.append((n, counts, notes, trace, identical))
    for n, counts, notes, trace, identical in details:
        print('\n### %s' % n)
        print('tags: ' + ', '.join('%s x%d' % kv for kv in sorted(counts.items())))
        print('JPEXS notes: ' + ('; '.join(notes) if notes else '(only PUBLISH_PROPERTY / EXPORT_SCOPE)'))
        print('Test Movie SWF identical to flash8.swf: ' +
              {True: 'yes', False: 'NO', None: '(Test Movie SWF not available)'}[identical])
        print('trace (flash8-trace.txt):')
        for t in (trace if trace is not None else ['(not captured)']):
            print('    ' + t)
    return 1 if bad else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
