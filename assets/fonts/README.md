# Embedded fonts

These fonts are compiled into Al-Jazari with `include_bytes!`. They are the
fallbacks for text whose face is not installed on the user's system (see
`docs/ARCHITECTURE.md`, section 8). They are **not** covered by the project's
Unlicense: each one is licensed under the SIL Open Font License 1.1, whose full
text, with the copyright notices of both upstream projects, is in
[`OFL.txt`](OFL.txt). Release bundles must ship `OFL.txt` with the executable.

All three are the static, unhinted TrueType builds (`glyf` outlines, no `fvar`
table), taken unchanged from the official Noto release archives on GitHub. Each
file starts with the TrueType magic `00 01 00 00`.

| File | Version | Size (bytes) | SHA-256 |
|---|---|---|---|
| `NotoSansArabic-Regular.ttf` | 2.013 | 142,140 | `bd86ca02f087d7f3c3788ba458fb6b73744c7639ed276b8d870dba6def6c40d0` |
| `NotoNaskhArabic-Regular.ttf` | 2.021 | 158,580 | `c34fdbd98af4dbc45ca192a23d2eeb77032add83086f5fe32957d23b3f36b221` |
| `NotoSans-Regular.ttf` | 2.015 | 431,364 | `f3961a9cde016d41a4879aecda1474d3a36d6bf54fa0e4643de029cc2248b0e8` |

## Sources

- **Noto Sans Arabic 2.013**: release `NotoSansArabic-v2.013` (2025-10-15) of
  <https://github.com/notofonts/arabic>, archive
  <https://github.com/notofonts/arabic/releases/download/NotoSansArabic-v2.013/NotoSansArabic-v2.013.zip>,
  member `NotoSansArabic/unhinted/ttf/NotoSansArabic-Regular.ttf`.
- **Noto Naskh Arabic 2.021**: release `NotoNaskhArabic-v2.021` (2026-01-16) of
  <https://github.com/notofonts/arabic>, archive
  <https://github.com/notofonts/arabic/releases/download/NotoNaskhArabic-v2.021/NotoNaskhArabic-v2.021.zip>,
  member `NotoNaskhArabic/unhinted/ttf/NotoNaskhArabic-Regular.ttf`.
- **Noto Sans 2.015** (Latin, Greek, Cyrillic and common punctuation, which the
  Arabic Noto fonts lack): release `NotoSans-v2.015` (2024-11-20) of
  <https://github.com/notofonts/latin-greek-cyrillic>, archive
  <https://github.com/notofonts/latin-greek-cyrillic/releases/download/NotoSans-v2.015/NotoSans-v2.015.zip>,
  member `NotoSans/unhinted/ttf/NotoSans-Regular.ttf`.

`OFL.txt` is the `OFL.txt` shipped in those archives: the licence text is the
same in both repositories, so it appears once, under both copyright lines.

## Updating

Download the new release archive, take the file from `unhinted/ttf/`, check
that it starts with `00 01 00 00` and has no `fvar` table, and update the
version, size and SHA-256 above. Fonts are data: never run anything that comes
inside a font archive.
