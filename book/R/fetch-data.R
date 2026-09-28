# book/R/fetch-data.R — the tables that are public records. Run it by hand;
# nothing sources it.
#
#     Rscript book/R/fetch-data.R
#
# `make-data.R` builds the tables this book invented or reshaped from R's own
# packages. This file builds the ones that are real records, downloaded from the
# agencies that publish them, so it is the only file in the book that needs the
# network. Like `world_borders`, what it writes is committed: a render reads
# the CSVs and never fetches (spec §20, "The cast is fetched, not shipped").
# `data/LICENSE.md` records where each table came from and on what terms.
#
# Each table is kept small on purpose. A reader downloads it in any of four
# languages, and the parity corpus carries a copy of every table the book reads,
# so the filters below are part of the design rather than tidying.

options(timeout = 1800)

.gog_out <- local({
  for (up in c(".", "..", "../..", "../../..")) {
    if (dir.exists(file.path(up, "gog-cli")))
      return(normalizePath(file.path(up, "book", "data"), mustWork = TRUE))
  }
  stop("fetch-data.R: cannot find the repository root from ", getwd())
})

# The same rule `make-data.R` follows: a file whose bytes did not change is left
# alone, because a running `quarto preview` reads a new timestamp in `data/` as
# a changed project and re-renders every chapter.
.gog_write <- function(frame, name) {
  path <- file.path(.gog_out, paste0(name, ".csv"))
  tmp <- tempfile(fileext = ".csv")
  write.csv(frame, tmp, row.names = FALSE, quote = TRUE, na = "")
  same <- file.exists(path) && file.size(path) == file.size(tmp) &&
    identical(readBin(path, "raw", file.size(path)), readBin(tmp, "raw", file.size(tmp)))
  if (!same) file.copy(tmp, path, overwrite = TRUE)
  unlink(tmp)
  cat(sprintf("%-12s %6d rows  %s\n", name, nrow(frame), if (same) "unchanged" else "written"))
}

# -- cyclones: every storm that reached hurricane strength, 1980 to 2025 --------
# NOAA's International Best Track Archive for Climate Stewardship (IBTrACS),
# version 4.01: every tropical cyclone's position and wind, merged from the
# agencies that track them. Five choices keep the table to about 30,000 rows:
#
#   * Storms whose one-minute wind reached 64 knots, hurricane strength. The
#     wind is `USA_WIND`, the US agencies' one-minute average, which exists for
#     every basin; the other agencies average over ten minutes, and mixing the
#     two would move storms between categories.
#   * Positions every twelve hours (0 and 12 UTC), from the six-hourly record.
#   * Only while the storm's wind was at least 34 knots, the speed at which a
#     storm is given a name. That leaves out the weak start of a storm and the
#     long tail after it has lost its strength.
#   * Whole years only, 1980 to 2025, so every calendar month covers 46 years.
#   * No `spur` tracks: those are a second record of a storm that merged or
#     split, and keeping them would draw part of a storm twice.
#
# `category` is the storm's highest category on the Saffir-Simpson scale, one
# value for the whole storm, in three bands: 64 to 95 knots is category 1 or 2,
# 96 to 136 is category 3 or 4, and 137 and over is category 5.
.ibtracs <- tempfile(fileext = ".csv")
download.file(paste0(
  "https://www.ncei.noaa.gov/data/international-best-track-archive-for-climate-",
  "stewardship-ibtracs/v04r01/access/csv/ibtracs.since1980.list.v04r01.csv"),
  .ibtracs, mode = "wb", quiet = TRUE)
.hdr <- names(read.csv(.ibtracs, nrows = 1, check.names = FALSE))
.keep <- c("SID", "NAME", "ISO_TIME", "LAT", "LON", "USA_WIND", "TRACK_TYPE")
.tc <- read.csv(.ibtracs, skip = 2, header = FALSE, col.names = .hdr,
                colClasses = ifelse(.hdr %in% .keep, "character", "NULL"),
                check.names = FALSE, na.strings = c("", " "))
unlink(.ibtracs)

.tc <- .tc[substr(.tc$ISO_TIME, 1, 4) >= "1980" & substr(.tc$ISO_TIME, 1, 4) <= "2025" &
             !startsWith(.tc$TRACK_TYPE, "spur"), ]
.tc$wind <- suppressWarnings(as.numeric(.tc$USA_WIND))
.peak <- tapply(.tc$wind, .tc$SID, function(w) suppressWarnings(max(w, na.rm = TRUE)))
.tc$peak_wind <- .peak[.tc$SID]
.tc <- .tc[is.finite(.tc$peak_wind) & .tc$peak_wind >= 64 & !is.na(.tc$wind) &
             .tc$wind >= 34 & substr(.tc$ISO_TIME, 12, 13) %in% c("00", "12"), ]
.tc <- .tc[order(.tc$SID, .tc$ISO_TIME), ]

# A storm is named by its name and the year it began, which is how a reader
# would look one up. A storm with no name, or whose name and year another storm
# also carries, keeps IBTrACS's own identifier, which is unique.
.first_year <- tapply(substr(.tc$ISO_TIME, 1, 4), .tc$SID, min)
.title <- function(s) {
  parts <- strsplit(tolower(s), "-", fixed = TRUE)
  vapply(parts, function(p) paste0(toupper(substring(p, 1, 1)), substring(p, 2),
                                   collapse = "-"), character(1))
}
.names <- tapply(.tc$NAME, .tc$SID, function(n) n[1])
.label <- ifelse(.names %in% c("NOT_NAMED", "UNNAMED") | is.na(.names),
                 names(.names), paste(.title(.names), .first_year[names(.names)]))
.clash <- .label %in% .label[duplicated(.label)]
.label[.clash] <- names(.names)[.clash]
names(.label) <- names(.names)

.bands <- c("1 or 2", "3 or 4", "5")
.lon <- as.numeric(.tc$LON)
cyclones <- data.frame(
  storm    = unname(.label[.tc$SID]),
  month    = month.abb[as.integer(substr(.tc$ISO_TIME, 6, 7))],
  category = .bands[findInterval(.tc$peak_wind, c(96, 137)) + 1],
  lon   = round(ifelse(.lon > 180, .lon - 360, .lon), 1),
  lat   = round(as.numeric(.tc$LAT), 1)
)
# Each storm's rows in time order, because `path` joins a group's rows in the
# order the table gives them. The weakest category comes first and category 5
# last, each oldest first, so the strongest storms are drawn on top.
.start <- tapply(.tc$ISO_TIME, .tc$SID, min)
cyclones <- cyclones[order(.tc$peak_wind >= 96, .tc$peak_wind >= 137,
                           .start[.tc$SID], .tc$SID, .tc$ISO_TIME), ]
stopifnot(!anyNA(cyclones), length(unique(cyclones$storm)) == length(.label))
.gog_write(cyclones, "cyclones")

# -- quakes_2011: every earthquake of magnitude 5 or more in 2011 --------------
# The US Geological Survey's earthquake catalog (ComCat), which records every
# earthquake of this size anywhere in the world. 2011 is the year of the
# magnitude 9.1 Tohoku earthquake off Japan and its aftershocks.
#
# The table is shaped for a sequence of weekly frames. A frame draws only its
# own rows, so an earthquake that is to stay visible for a while needs a row in
# each frame that shows it. Every earthquake therefore has four rows: one in
# the week it struck, with `age` "this week", and one in each of the next three
# weeks, "1 week ago" to "3 weeks ago". `week` is the first day of a seven-day
# week counted from 1 January; the fifty-third holds only 31 December.
.q <- read.csv(paste0(
  "https://earthquake.usgs.gov/fdsnws/event/1/query?format=csv",
  "&starttime=2011-01-01&endtime=2012-01-01&minmagnitude=5&orderby=time-asc"),
  stringsAsFactors = FALSE)
.day <- as.Date(substr(.q$time, 1, 10))
.wk <- as.integer(.day - as.Date("2011-01-01")) %/% 7
quakes_2011 <- do.call(rbind, lapply(0:3, function(ago) data.frame(
  week      = format(as.Date("2011-01-01") + 7 * (.wk + ago)),
  lon       = round(.q$longitude, 2),
  lat       = round(.q$latitude, 2),
  magnitude = .q$mag,
  age       = c("this week", "1 week ago", "2 weeks ago", "3 weeks ago")[ago + 1],
  ago       = ago,
  when      = .day
)))
quakes_2011 <- quakes_2011[.wk[rep(seq_len(nrow(.q)), 4)] + quakes_2011$ago <= 52, ]
# Within a week the older earthquakes come first, so the newest are drawn on top.
quakes_2011 <- quakes_2011[order(quakes_2011$week, -quakes_2011$ago, quakes_2011$when), ]
quakes_2011$ago <- NULL
quakes_2011$when <- NULL
stopifnot(!anyNA(quakes_2011), sum(quakes_2011$age == "this week") == nrow(.q))
.gog_write(quakes_2011, "quakes_2011")
