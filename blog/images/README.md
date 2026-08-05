# The blog's images

**`og.svg` is the source; `og.png` is what ships.** The card is rebuilt with the
same converter the book uses for its PDF figures:

```bash
rsvg-convert -w 1200 -h 630 og.svg -o og.png
```

`og.svg` references `gog_hex.png` by relative path, so both files have to sit in
this directory for the conversion to work.

**1200 x 630, and the ratio is the requirement rather than the size.** Open Graph
and Twitter both expect about 1.91:1. The hex sticker itself is 1002 x 1150 and
portrait, so it cannot be the card: a portrait image is letterboxed by Bluesky
and X falls back to a small square thumbnail instead of a banner. The hex stays
the favicon, and it is the source art inside the card.

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
