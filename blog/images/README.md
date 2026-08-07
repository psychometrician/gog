# The blog's images

**`og.svg` is the source; `og.png` is what ships.** The card is rebuilt with the
same converter the book uses for its PDF figures:

```bash
rsvg-convert -w 1200 -h 630 og.svg -o og.png
```

`og.svg` references `gog-hex.png` by relative path, so both files have to sit in
this directory for the conversion to work.

**1200 x 630, and the ratio is the requirement rather than the size.** Open Graph
and Twitter both expect about 1.91:1. The hex sticker is portrait, so it cannot
be the card: a portrait image is letterboxed by Bluesky and X falls back to a
small square thumbnail instead of a banner. The hex stays the favicon, and it is
the source art inside the card.

**`gog-hex.png` is the sticker, and it is raster on purpose.** Every mark on the
face is geometry and would survive as vector, but the name across the bottom is
type, set in Avenir Next. That font ships on macOS and almost nowhere else, so a
vector sticker draws its own wordmark differently for most of the people who see
it. Rendering it once fixes the letterform for everybody. The same file is the
favicon, the front page's hero, the book's cover, the repository's README and
the art inside the card here.

Hacker News renders no card at all. It shows a title and a domain, so none of
this reaches it.

**Each post carries its own `card.png`.** Both currently hold a copy of `og.png`,
which is the sensible default and not a permanent arrangement: a post about
polar plots wants a wind rose on its card. Replace the copy in the post's own
directory and nothing else has to change, because a post's `image:` overrides the
site default in the page metadata, in the listing thumbnail and in the feed item
alike.

That last one is the reason every post should have one. R-bloggers republishes
through WordPress, whose sanitizer is expected to strip the inline `<svg>` that
every plot here is made of, so the card may be the only picture that survives
into the syndicated copy.
