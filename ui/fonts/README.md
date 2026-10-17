# Literata

The typeface for scripture and chapter headings, by TypeTogether, under the
SIL Open Font License 1.1 (`OFL.txt`, beside the fonts). The licence has no
Reserved Font Name, so the cut-down files keep the name.

Source: `ofl/literata/` in [google/fonts](https://github.com/google/fonts)
at commit `4e5f06dbb274a27ebe71ed54ea706b3ee40eabd9`, version 3.103:

| Source file | SHA-256 |
|---|---|
| `Literata[opsz,wght].ttf` | `b41138c9373112f32abb589cc22e8674b06ed4048b0c513be922bdd26f274440` |
| `Literata-Italic[opsz,wght].ttf` | `d483dfaeba9cbf4ce71d32a52ee65df82f7e35b15fff8d1011cdb242d1fcd465` |

Each file here is one static instance at the text optical size (`opsz=12`),
cut to Google Fonts' "latin" range (the characters the KJV's, BSB's and
WEB's text use) and saved as WOFF2 with fontTools 4.66:

| File | Instance |
|---|---|
| `literata-regular.woff2` | `Literata[opsz,wght].ttf`, `wght=400` |
| `literata-italic.woff2` | `Literata-Italic[opsz,wght].ttf`, `wght=400` (the real italic, for the KJV's supplied words) |
| `literata-bold.woff2` | `Literata[opsz,wght].ttf`, `wght=700` |

```sh
U="U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+0304,U+0308,U+0329,U+2000-206F,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD"
fonttools varLib.instancer -o r400.ttf 'Literata[opsz,wght].ttf' opsz=12 wght=400 --update-name-table
pyftsubset r400.ttf --unicodes="$U" --flavor=woff2 --no-hinting --desubroutinize \
    --layout-features='kern,liga,clig,calt,ccmp,locl,mark,mkmk,onum,lnum,pnum,tnum,case,frac,sups' \
    --output-file=literata-regular.woff2
```

and the same for the italic and the bold. `ui/index.html`'s `@font-face`
rules give the same range, so any other character falls back to the next
face in `--font-scripture`.
