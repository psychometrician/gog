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

# -- us_counties: every county of the lower 48 states and DC, by poverty --------
# Two Census Bureau files, joined by each county's FIPS code:
#
#   * The 2023 cartographic boundary file at 1:20,000,000, the Bureau's own
#     simplified county outlines for maps of the whole country.
#   * The 2023 Small Area Income and Poverty Estimates (SAIPE). `poverty` is the
#     estimated percent of people of all ages living in poverty.
#
# Alaska, Hawaii and Puerto Rico are left out. Drawn where they are, they would
# shrink the lower 48 to a strip, and a map draws every place where it is.
#
# The shapefile is read without a spatial package: a polygon record is a list of
# rings, and its attributes sit in a fixed-width dBase file beside it. Each ring
# is simplified to a twentieth of a degree, about five kilometers and less than
# a pixel on the book's map, by the Douglas-Peucker rule. A ring that would
# close up at that tolerance is kept at a thousandth of a degree, or whole, so
# the smallest counties, such as Virginia's independent cities, keep their
# outline. That keeps the table near 24,000 rows. `piece` names the rings, as
# in `world_borders`, as text, because `group` takes a category. A county's name is the Bureau's full one with
# its state, "Autauga County, AL", because the short name repeats: Virginia has a
# Richmond city and a Richmond County.
.cb <- file.path(tempdir(), "cb_2023_us_county_20m")
download.file(
  "https://www2.census.gov/geo/tiger/GENZ2023/shp/cb_2023_us_county_20m.zip",
  paste0(.cb, ".zip"), mode = "wb", quiet = TRUE)
unzip(paste0(.cb, ".zip"), exdir = .cb)

.le <- function(r, size) readBin(r, "integer", size = size, signed = size > 2,
                                 endian = "little")
.read_dbf <- function(path) {
  b <- readBin(path, "raw", file.size(path))
  n <- .le(b[5:8], 4); hlen <- .le(b[9:10], 2); rlen <- .le(b[11:12], 2)
  widths <- integer(); at <- 33
  while (b[at] != as.raw(0x0D)) {
    name <- b[at:(at + 10)]
    widths[rawToChar(name[name != as.raw(0)])] <- as.integer(b[at + 16])
    at <- at + 32
  }
  # Each record opens with a one-byte deletion flag, then the fields in order.
  first <- hlen + (seq_len(n) - 1) * rlen + 2
  offset <- c(0, cumsum(widths))[seq_along(widths)]
  out <- lapply(seq_along(widths), function(k) {
    v <- vapply(first, function(s)
      rawToChar(b[(s + offset[k]):(s + offset[k] + widths[k] - 1)]), "")
    Encoding(v) <- "UTF-8"
    trimws(v)
  })
  names(out) <- names(widths)
  as.data.frame(out, stringsAsFactors = FALSE)
}
.read_shp <- function(path) {
  b <- readBin(path, "raw", file.size(path))
  shapes <- list(); at <- 101
  while (at < length(b)) {
    len <- readBin(b[(at + 4):(at + 7)], "integer", size = 4, endian = "big") * 2
    body <- at + 8
    rings <- list()
    if (.le(b[body:(body + 3)], 4) == 5) {
      np <- .le(b[(body + 36):(body + 39)], 4)
      nv <- .le(b[(body + 40):(body + 43)], 4)
      parts <- readBin(b[(body + 44):(body + 43 + 4 * np)], "integer", size = 4,
                       n = np, endian = "little")
      xy <- readBin(b[(body + 44 + 4 * np):(body + 43 + 4 * np + 16 * nv)],
                    "double", size = 8, n = 2 * nv, endian = "little")
      ends <- c(parts[-1], nv)
      rings <- lapply(seq_len(np), function(k) {
        i <- (2 * parts[k] + 1):(2 * ends[k])
        cbind(xy[i][c(TRUE, FALSE)], xy[i][c(FALSE, TRUE)])
      })
    }
    shapes[[length(shapes) + 1]] <- rings
    at <- body + len
  }
  shapes
}
# Douglas-Peucker on a closed ring: split at the point farthest from the first,
# so neither half is a segment from a point to itself, then keep a point only if
# it lies farther than `tol` from the segment its neighbors would draw.
.simplify <- function(p, tol) {
  n <- nrow(p)
  if (n < 4) return(p)
  keep <- logical(n)
  far <- which.max((p[, 1] - p[1, 1])^2 + (p[, 2] - p[1, 2])^2)
  keep[c(1, far, n)] <- TRUE
  todo <- list(c(1, far), c(far, n))
  while (length(todo)) {
    ab <- todo[[length(todo)]]; todo[[length(todo)]] <- NULL
    a <- ab[1]; z <- ab[2]
    if (z - a < 2) next
    i <- (a + 1):(z - 1)
    dx <- p[z, 1] - p[a, 1]; dy <- p[z, 2] - p[a, 2]
    d <- abs(dy * p[i, 1] - dx * p[i, 2] + p[z, 1] * p[a, 2] - p[z, 2] * p[a, 1]) /
      sqrt(dx^2 + dy^2)
    k <- which.max(d)
    if (d[k] > tol) {
      keep[i[k]] <- TRUE
      todo <- c(todo, list(c(a, i[k]), c(i[k], z)))
    }
  }
  p[keep, , drop = FALSE]
}

.attrs <- .read_dbf(file.path(.cb, "cb_2023_us_county_20m.dbf"))
.shapes <- .read_shp(file.path(.cb, "cb_2023_us_county_20m.shp"))
stopifnot(length(.shapes) == nrow(.attrs))

.saipe <- readLines(paste0(
  "https://www2.census.gov/programs-surveys/saipe/datasets/2023/",
  "2023-state-and-county/est23all.txt"), encoding = "latin1")
.fips <- sprintf("%02d%03d", as.integer(substr(.saipe, 1, 2)),
                 as.integer(substr(.saipe, 4, 6)))
.poverty <- setNames(suppressWarnings(as.numeric(substr(.saipe, 35, 38))), .fips)

.lower <- order(.attrs$GEOID)
.lower <- .lower[!(.attrs$STATEFP[.lower] %in% c("02", "15", "72"))]
.piece <- 0L
us_counties <- do.call(rbind, lapply(.lower, function(j) {
  rings <- lapply(.shapes[[j]], function(r) {
    kept <- .simplify(r, tol = 0.05)
    if (nrow(kept) < 4) kept <- .simplify(r, tol = 0.001)
    if (nrow(kept) < 4) r else kept
  })
  rings <- rings[vapply(rings, nrow, 1L) >= 4]
  do.call(rbind, lapply(rings, function(r) {
    .piece <<- .piece + 1L
    data.frame(lon = round(r[, 1], 3), lat = round(r[, 2], 3),
               county = paste0(.attrs$NAMELSAD[j], ", ", .attrs$STUSPS[j]),
               piece = sprintf("p%05d", .piece),
               poverty = unname(.poverty[.attrs$GEOID[j]]))
  }))
}))
stopifnot(!anyNA(us_counties),
          length(unique(us_counties$county)) == length(.lower))
.gog_write(us_counties, "us_counties")

# -- ohio_turnout: the share of each Ohio county's citizens who voted -----------
# Three presidential elections, 2016, 2020 and 2024, from two federal sources
# joined by each county's FIPS code, drawn on the outlines `us_counties` reads:
#
#   * The Election Administration and Voting Survey (EAVS) of the US Election
#     Assistance Commission. Each state reports, for each county, how many
#     people voted: item F1a, the ballots counted. Every one of Ohio's 88
#     counties reports it in all three years.
#   * The Census Bureau's citizen voting-age population (CVAP), a special
#     tabulation of the American Community Survey. Each estimate averages the
#     five years that end in the election year, 2012 to 2016 for 2016.
#
# `turnout` is the ballots counted as a percent of the citizens aged 18 and
# over. The count of citizens is a five-year average, so a county that grew
# quickly in those years comes out a little higher than it was. The outlines
# are simplified to a two-hundredth of a degree, finer than `us_counties`,
# because one state is drawn much larger than the whole country.
.eavs <- function(url, member) {
  zip <- tempfile(fileext = ".zip")
  download.file(url, zip, mode = "wb", quiet = TRUE)
  e <- read.csv(unzip(zip, files = member, exdir = tempfile()),
                colClasses = "character", fileEncoding = "latin1")
  state <- if ("State_Abbr" %in% names(e)) e$State_Abbr else e$State
  # A county's row has a ten-digit code ending in zeros. Some states report
  # towns instead, under longer codes, and Ohio does not.
  e <- e[state == "OH" & nchar(e$FIPSCode) == 10 & endsWith(e$FIPSCode, "00000"), ]
  setNames(as.numeric(e$F1a), substr(e$FIPSCode, 1, 5))
}
.cvap <- function(url) {
  zip <- tempfile(fileext = ".zip")
  download.file(url, zip, mode = "wb", quiet = TRUE)
  v <- read.csv(unzip(zip, files = "County.csv", exdir = tempfile()),
                colClasses = "character", fileEncoding = "latin1")
  names(v) <- tolower(names(v))
  v <- v[v$lntitle == "Total", ]
  setNames(as.numeric(v$cvap_est), substr(v$geoid, nchar(v$geoid) - 4, nchar(v$geoid)))
}
.eac <- "https://www.eac.gov/sites/default/files/"
.acs <- "https://www2.census.gov/programs-surveys/decennial/rdo/datasets/"
.elections <- list(
  "2016" = c(paste0(.eac, "2023-12/EAVS_2016_for_Public_Release_nolabel_V1.1_CSV.zip"),
             "EAVS_2016_Final_Data_for_Public_Release_nolabel_V1.1_CSV.csv",
             paste0(.acs, "2016/2016-cvap/CVAP_2012-2016_ACS_csv_files.zip")),
  "2020" = c(paste0(.eac, "2023-12/2020_EAVS_for_Public_Release_nolabel_V1.2_CSV.zip"),
             "2020_EAVS_for_Public_Release_nolabel_V1.2_CSV.csv",
             paste0(.acs, "2020/2020-cvap/CVAP_2016-2020_ACS_csv_files.zip")),
  "2024" = c(paste0(.eac, "2026-02/2024_EAVS_for_Public_Release_nolabel_V2_csv.zip"),
             "2024_EAVS_for_Public_Release_nolabel_V2.csv",
             paste0(.acs, "2024/2024-cvap/CVAP_2020-2024_ACS_csv_files.zip")))
.turnout <- lapply(.elections, function(e) {
  votes <- .eavs(e[1], e[2])
  stopifnot(length(votes) == 88, all(votes > 0))
  100 * votes / .cvap(e[3])[names(votes)]
})

.ohio <- order(.attrs$GEOID)
.ohio <- .ohio[.attrs$STATEFP[.ohio] == "39"]
.rings <- lapply(.ohio, function(j) lapply(.shapes[[j]], function(r) {
  kept <- .simplify(r, tol = 0.005)
  if (nrow(kept) < 4) r else kept
}))
ohio_turnout <- do.call(rbind, lapply(names(.elections), function(year) {
  .piece <- 0L
  do.call(rbind, lapply(seq_along(.ohio), function(k) {
    j <- .ohio[k]
    do.call(rbind, lapply(.rings[[k]], function(r) {
      .piece <<- .piece + 1L
      data.frame(lon = round(r[, 1], 3), lat = round(r[, 2], 3),
                 county = .attrs$NAMELSAD[j], piece = sprintf("p%03d", .piece),
                 election = year,
                 turnout = round(unname(.turnout[[year]][.attrs$GEOID[j]]), 1))
    }))
  }))
}))
stopifnot(!anyNA(ohio_turnout), length(unique(ohio_turnout$county)) == 88)
.gog_write(ohio_turnout, "ohio_turnout")
