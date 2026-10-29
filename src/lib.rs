//! Product Hunt Today: the day's top launches as a 1080x1080 countdown.
//!
//! The data (`data/products.json`, logos and gallery images in `data/media/`) is written by
//! `scripts/fetch.py` and read at runtime, so a new day only needs a new fetch.
//!
//! The whole video is one ranked table. Each launch opens out of its own row into a full card
//! (the row's surface becomes the field), shows its gallery, then folds back into the table,
//! which now carries its vote count. After #1 the table becomes the leaderboard.
//!
//! Timing follows the music (160 BPM half-time, one bar = 1.5 s): a card is fully open on the
//! downbeat of its four-bar block.
use fframes::{
    AudioMap, AudioTimestamp::*, AudioTrack, Color, Duration, FFramesContext, FontQuery, Frame,
    Scene, Scenes, Svgr, TextOverflow, Video,
    animation::{AnimationRuntime, Easing},
    include_media_dir,
};
use serde::Deserialize;
use std::{path::Path, sync::LazyLock, sync::OnceLock};

// Fonts. Sounds (`audio/`, kept stereo) and the day's images (`data/media/`) load at runtime.
include_media_dir!(pub struct PhTodayMedia, "media");

pub const WIDTH: usize = 1080;
pub const HEIGHT: usize = 1080;

const BEAT: f32 = 60.0 / 160.0;
const INTRO: f32 = 3.0;
const PER_PRODUCT: f32 = 6.0;
const OUTRO: f32 = 12.0 * BEAT;
/// Row to card: the card is fully open on its cue.
const OPEN: f32 = 0.6;
/// When a card starts folding back, relative to its cue.
const CLOSE_AT: f32 = 4.6;
/// When the vote count starts rolling, relative to the cue.
const COUNT_AT: f32 = 0.15;
/// Gallery changes for one, two or three images, on beats after the cue.
const SHOT_SWITCHES: [&[f32]; 3] = [&[0.0], &[0.0, 6.0 * BEAT], &[0.0, 4.0 * BEAT, 8.0 * BEAT]];

// One ground, one ink, tone steps instead of lines, and Product Hunt's own orange only on
// the upvote marks. The winner's row is the one inverted field.
const GROUND: &str = "#ffffff";
const SURFACE: &str = "#f2f1ee";
const SURFACE_BAR: &str = "#e4e2dc";
const INK: &str = "#141414";
const MUTED: &str = "#66635e";
const ON_INK: &str = "#ffffff";
const ON_INK_MUTED: &str = "#a9a59e";
const UPVOTE: &str = "#da552f";
const FONT: &str = "Schibsted Grotesk";

// Three sizes do all the work.
const DISPLAY: usize = 96;
const TEXT: usize = 44;
const SMALL: usize = 28;

const MARGIN: f32 = 72.0;
const INNER: f32 = 1080.0 - 2.0 * MARGIN;

// The table.
const ROWS_TOP: f32 = 292.0;
const ROW_H: f32 = 132.0;
const ROW_GAP: f32 = 14.0;
const ROW_RANK_X: f32 = MARGIN + 32.0;
const ROW_LOGO_X: f32 = MARGIN + 124.0;
const ROW_LOGO: f32 = 76.0;
const ROW_NAME_X: f32 = MARGIN + 232.0;
const ROW_COUNT_RIGHT: f32 = MARGIN + INNER - 32.0;
const ROW_NAME_W: f32 = 500.0;

// An open card.
const CARD_LOGO: f32 = 112.0;
const CARD_COUNT_X: f32 = MARGIN + CARD_LOGO + 64.0;
const CARD_NAME_Y: f32 = 294.0;
const CARD_TAGLINE_Y: f32 = 356.0;
const CARD_TAGLINE_LEAD: f32 = 48.0;
const TAGLINE: usize = 40;
/// The gallery sits inside the margins like the text: full width, 5:3 (the size fetch.py pads
/// every image to), bottom on the bottom margin.
const SHOT_H: f32 = INNER * 0.6;
const SHOT_Y: f32 = 1080.0 - MARGIN - SHOT_H;

// ------------------------------------------------------------------ data

#[derive(Debug, Clone, Deserialize)]
pub struct Product {
    pub rank: usize,
    pub name: String,
    pub tagline: String,
    pub votes: u32,
    pub logo: Option<String>,
    #[serde(default)]
    pub shots: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Board {
    /// `daily` (yesterday's featured launches) or `today` (trending so far).
    pub mode: String,
    pub date_label: String,
    pub products: Vec<Product>,
}

impl Board {
    pub fn load(data_dir: &Path) -> Result<Self, String> {
        let path = data_dir.join("products.json");
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("{}: {e}. Run `python scripts/fetch.py` first.", path.display()))?;
        let mut board: Board = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        board.products.truncate(5);
        if board.products.is_empty() {
            return Err(format!("{}: no products", path.display()));
        }
        Ok(board)
    }

    fn max_votes(&self) -> f32 {
        self.products.iter().map(|p| p.votes).max().unwrap_or(1).max(1) as f32
    }

    fn headline(&self) -> String {
        if self.mode == "today" {
            "Trending today".to_owned()
        } else {
            format!("Yesterday\u{2019}s top {}", self.products.len())
        }
    }

    fn total(&self) -> f32 {
        INTRO + self.products.len() as f32 * PER_PRODUCT + OUTRO
    }
}

/// Video time at which the card of the product at `index` (0 = #1) is fully open.
fn cue_of(index: usize, count: usize) -> f32 {
    INTRO + (count - 1 - index) as f32 * PER_PRODUCT
}

// ------------------------------------------------------------------ motion helpers

static OUT_FAST: LazyLock<AnimationRuntime> = LazyLock::new(|| AnimationRuntime::new(0.35, &Easing::EaseOut));
static OUT: LazyLock<AnimationRuntime> = LazyLock::new(|| AnimationRuntime::new(0.6, &Easing::EaseOut));
static OUT_LONG: LazyLock<AnimationRuntime> = LazyLock::new(|| AnimationRuntime::new(1.1, &Easing::EaseOut));
static MORPH: LazyLock<AnimationRuntime> = LazyLock::new(|| AnimationRuntime::new(OPEN, &Easing::EaseInOut));
static PUSH: LazyLock<AnimationRuntime> = LazyLock::new(|| AnimationRuntime::new(0.6, &Easing::EaseInOut));
static LEAVE: LazyLock<AnimationRuntime> = LazyLock::new(|| AnimationRuntime::new(0.22, &Easing::EaseIn));
static COUNT: LazyLock<AnimationRuntime> = LazyLock::new(|| AnimationRuntime::new(1.4, &Easing::EaseOut));
static SPRING: LazyLock<AnimationRuntime> = LazyLock::new(|| {
    AnimationRuntime::new(3.0, &Easing::Spring { mass: 1.0, stiffness: 170.0, damping: 22.0 })
});

/// Progress 0..1 of `runtime` started at `start` (springs overshoot past 1 on the way).
fn ease(runtime: &AnimationRuntime, t: f32, start: f32) -> f32 {
    let local = t - start;
    if local <= 0.0 {
        0.0
    } else if local >= runtime.get_duration() {
        1.0
    } else {
        runtime.solve(&local)
    }
}

fn lerp(a: f32, b: f32, p: f32) -> f32 {
    a + (b - a) * p
}

/// Rises from `distance` px below while fading in: (translate y, opacity).
fn rise(t: f32, start: f32, distance: f32) -> (f32, f32) {
    (lerp(distance, 0.0, ease(&SPRING, t, start)), ease(&OUT_FAST, t, start))
}

fn font(size: usize, weight: u16) -> FontQuery<'static> {
    FontQuery { family: FONT, size, weight, ..Default::default() }
}

// Measured text has to borrow for the context's lifetime `'a`, hence the explicit lifetimes.

/// Largest size from `max` down to `min` at which `text` fits `width`.
fn fit_size<'a>(frame: &mut Frame, ctx: &FFramesContext<'a, '_>, text: &'a str, weight: u16, max: usize, min: usize, width: f32) -> usize {
    let mut size = max;
    while size > min {
        match frame.text_width(ctx, font(size, weight), text) {
            Some(w) if w as f32 > width => size -= 2,
            _ => break,
        }
    }
    size
}

/// Fits on one line, with an ellipsis when even the smallest size is too wide.
fn fit_line<'a>(frame: &mut Frame, ctx: &FFramesContext<'a, '_>, text: &'a str, f: FontQuery, width: f32) -> String {
    frame
        .text_fit(ctx, f, text, width as usize, TextOverflow::Ellipsis)
        .map(|t| t.into_owned())
        .unwrap_or_else(|| text.to_owned())
}

/// Greedy word wrap into at most `max_lines`; the last line ends in an ellipsis if text is
/// left over. Words are measured one at a time so every measured string borrows from `text`.
fn wrap<'a>(frame: &mut Frame, ctx: &FFramesContext<'a, '_>, text: &'a str, f: FontQuery, width: f32, max_lines: usize) -> Vec<String> {
    let width = width as usize;
    let space = frame.text_width(ctx, f, "a a").unwrap_or(0).saturating_sub(2 * frame.text_width(ctx, f, "a").unwrap_or(0));
    let ellipsis = frame.text_width(ctx, f, " …").unwrap_or(0);
    let words: Vec<(&'a str, usize)> = text
        .split_whitespace()
        .map(|w| (w, frame.text_width(ctx, f, w).unwrap_or(0)))
        .collect();
    let line_w = |line: &[(&str, usize)]| line.iter().map(|(_, w)| w).sum::<usize>() + space * line.len().saturating_sub(1);

    let mut lines: Vec<Vec<(&str, usize)>> = vec![vec![]];
    let mut overflow = false;
    for &word in &words {
        let current = lines.last_mut().unwrap();
        if current.is_empty() || line_w(current) + space + word.1 <= width {
            current.push(word);
        } else if lines.len() < max_lines {
            lines.push(vec![word]);
        } else {
            overflow = true;
            break;
        }
    }
    if overflow {
        let last = lines.last_mut().unwrap();
        while last.len() > 1 && line_w(last) + ellipsis > width {
            last.pop();
        }
    }
    let count = lines.len();
    lines
        .into_iter()
        .enumerate()
        .map(|(i, line)| {
            let text = line.iter().map(|(w, _)| *w).collect::<Vec<_>>().join(" ");
            if overflow && i == count - 1 { format!("{text} …") } else { text }
        })
        .collect()
}

/// Like `wrap`, but narrows the measure as far as the line count allows so two lines come out
/// about even instead of leaving one word alone on the second (CSS `text-wrap: balance`).
fn balanced<'a>(frame: &mut Frame, ctx: &FFramesContext<'a, '_>, text: &'a str, f: FontQuery, width: f32, max_lines: usize) -> Vec<String> {
    let lines = wrap(frame, ctx, text, f, width, max_lines);
    if lines.len() < 2 || lines.last().is_some_and(|l| l.ends_with('…')) {
        return lines;
    }
    let mut best = lines;
    let mut w = width - 10.0;
    while w > width * 0.4 {
        let candidate = wrap(frame, ctx, text, f, w, max_lines);
        if candidate.len() != best.len() || candidate.last().is_some_and(|l| l.ends_with('…')) {
            break;
        }
        best = candidate;
        w -= 10.0;
    }
    best
}

/// Digits in fixed-width cells so a rolling counter does not jiggle; starts at `x`.
fn tabular<'a>(value: u32, x: f32, y: f32, cell: f32, size: usize, weight: u16, fill: &'a str) -> Svgr<'a> {
    let glyphs: Vec<Svgr> = value
        .to_string()
        .chars()
        .enumerate()
        .map(|(i, ch)| {
            let cx = x + (i as f32 + 0.5) * cell;
            fframes::svgr!(<text x={cx} y={y} text-anchor="middle" font-family={FONT} font-size={size} font-weight={weight} fill={fill}>{ch.to_string()}</text>)
        })
        .collect();
    fframes::svgr!(<g>{glyphs}</g>)
}

/// Product Hunt's upvote mark, a small triangle, with its left edge at `x`.
fn upvote<'a>(x: f32, baseline: f32, size: f32) -> Svgr<'a> {
    let h = size * 0.86;
    let bottom = baseline - size * 0.06;
    let d = format!("M{} {} L{} {} L{} {} Z", x + size / 2.0, bottom - h, x + size, bottom, x, bottom);
    fframes::svgr!(<path d={d} fill={UPVOTE} />)
}

/// The product's logo, square, on white (many logos are transparent).
fn logo<'a>(ctx: &FFramesContext<'a, '_>, product: &'a Product, x: f32, y: f32, size: f32) -> Svgr<'a> {
    let image = product
        .logo
        .as_ref()
        .and_then(|name| ctx.get_image(name))
        .map(|img| fframes::svgr!(<image href={img.href()} x={x} y={y} width={size} height={size} preserveAspectRatio="xMidYMid slice" />))
        .unwrap_or_else(|| {
            let initial = product.name.chars().next().unwrap_or('?').to_uppercase().to_string();
            fframes::svgr!(<text x={x + size / 2.0} y={y + size * 0.7} text-anchor="middle" font-family={FONT} font-weight="700" font-size={size * 0.6} fill={INK}>{initial}</text>)
        });
    fframes::svgr!(<g>
        <rect x={x} y={y} width={size} height={size} fill={GROUND} />
        {image}
    </g>)
}

// ------------------------------------------------------------------ the video

/// Measured once on the first frame.
#[derive(Debug)]
struct Layout {
    /// Per product (index 0 = #1): card name, its size, and the scale that fits it in a row.
    names: Vec<(String, usize, f32)>,
    /// Width of "#1".."#5" at display size, for right-aligning the card's rank.
    rank_w: Vec<f32>,
    taglines: Vec<Vec<String>>,
    headline: String,
    cell: f32,
}

/// A named stretch of the timeline. The video draws one continuous table, so the parts only
/// name the stretches for the command line (`strip Rank3`, `preview Leaderboard`).
#[derive(Debug)]
struct Part {
    name: &'static str,
    seconds: f32,
}

impl Scene for Part {
    fn duration(&self) -> Duration<'_> {
        Duration::Seconds(self.seconds)
    }

    fn name(&self) -> &'static str {
        self.name
    }

    fn render_frame<'a>(&'a self, _frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        Svgr::empty()
    }
}

const RANK_LABELS: [&str; 5] = ["#1", "#2", "#3", "#4", "#5"];
const RANK_PARTS: [&str; 5] = ["Rank1", "Rank2", "Rank3", "Rank4", "Rank5"];

pub struct PhTodayVideo<'a> {
    board: &'a Board,
    parts: Vec<Part>,
    layout: OnceLock<Layout>,
}

impl<'a> PhTodayVideo<'a> {
    pub fn new(board: &'a Board) -> Self {
        let n = board.products.len();
        let mut parts = vec![Part { name: "Intro", seconds: INTRO }];
        parts.extend((0..n).rev().map(|i| Part { name: RANK_PARTS[i], seconds: PER_PRODUCT }));
        parts.push(Part { name: "Leaderboard", seconds: OUTRO });
        Self { board, parts, layout: OnceLock::new() }
    }

    fn layout(&'a self, frame: &mut Frame, ctx: &FFramesContext<'a, '_>) -> &'a Layout {
        self.layout.get_or_init(|| {
            let names = self
                .board
                .products
                .iter()
                .map(|p| {
                    let size = fit_size(frame, ctx, &p.name, 700, DISPLAY, 60, INNER);
                    let text = fit_line(frame, ctx, &p.name, font(size, 700), INNER);
                    // The row shows the same text, scaled down to the row size or to fit the row.
                    let width = frame.text_width(ctx, font(size, 700), &p.name).unwrap_or(1).min(INNER as usize) as f32;
                    let scale = (TEXT as f32 / size as f32).min(ROW_NAME_W / width);
                    (text, size, scale)
                })
                .collect();
            let rank_w = (0..self.board.products.len())
                .map(|i| frame.text_width(ctx, font(DISPLAY, 700), RANK_LABELS[i]).unwrap_or(100) as f32)
                .collect();
            let taglines = self
                .board
                .products
                .iter()
                .map(|p| balanced(frame, ctx, &p.tagline, font(TAGLINE, 500), INNER, 2))
                .collect();
            let cell = frame.text_width(ctx, font(TEXT, 500), "0").unwrap_or(26) as f32 * 1.02;
            Layout { names, rank_w, taglines, headline: self.board.headline(), cell }
        })
    }

    fn row_y(i: usize) -> f32 {
        ROWS_TOP + i as f32 * (ROW_H + ROW_GAP)
    }

    /// One product, from its table row (`open` = 0) to its full card (`open` = 1).
    fn product<'b>(&'b self, ctx: &FFramesContext<'b, '_>, layout: &'b Layout, i: usize, t: f32) -> Svgr<'b> {
        let n = self.board.products.len();
        let p = &self.board.products[i];
        let cue = cue_of(i, n);
        let open = ease(&MORPH, t, cue - OPEN) - ease(&MORPH, t, cue + CLOSE_AT);
        // Card-only parts: in once the card is open, out just before it folds.
        let card = ease(&OUT_FAST, t, cue - 0.05) * (1.0 - ease(&LEAVE, t, cue + CLOSE_AT - 0.2));
        let winner = p.rank == 1;
        let (field, ink, muted, bar) = if winner {
            (INK, ON_INK, ON_INK_MUTED, INK)
        } else {
            (SURFACE, INK, MUTED, SURFACE_BAR)
        };
        let row_y = Self::row_y(i);
        let (name, name_size, row_scale) = &layout.names[i];

        // The row's surface grows into the whole frame, ahead of what it carries, so the
        // contents never stick out of it.
        let lead = 1.0 - (1.0 - open) * (1.0 - open);
        let fx = lerp(MARGIN, 0.0, lead);
        let fy = lerp(row_y, 0.0, lead);
        let fw = lerp(INNER, 1080.0, lead);
        let fh = lerp(ROW_H, 1080.0, lead);

        // Leaderboard: the row fills with its share of the votes, #5 first.
        let board_start = cue_of(0, n) + CLOSE_AT + OPEN + 0.2;
        let grow = ease(&OUT_LONG, t, board_start + (n - 1 - i) as f32 * 0.08);
        let bar = if grow > 0.0 {
            let w = (INNER * p.votes as f32 / self.board.max_votes() * grow).max(0.5);
            fframes::svgr!(<rect x={MARGIN} y={row_y} width={w} height={ROW_H} fill={bar} />)
        } else {
            Svgr::empty()
        };

        // Shared parts move between their row and card positions.
        // Only the logo travels. The row's text rides with it while fading in the first third;
        // the card's text rises into place once the surface has covered the frame.
        let row_logo_y = row_y + (ROW_H - ROW_LOGO) / 2.0;
        let logo_size = lerp(ROW_LOGO, CARD_LOGO, lead);
        let logo_x = lerp(ROW_LOGO_X, MARGIN, lead);
        let logo_y = lerp(row_logo_y, MARGIN, lead);
        let (ride_x, ride_y) = (logo_x - ROW_LOGO_X, logo_y - row_logo_y);
        let row_o = 1.0 - (open / 0.3).clamp(0.0, 1.0);
        // Card text waits until the logo is home, above the name line.
        let card_o = ((lead - 0.93) / 0.07).clamp(0.0, 1.0);
        let card_dy = (1.0 - card_o) * 18.0;

        let card_rank_x = MARGIN + INNER - layout.rank_w[i];
        let rank_scale = TEXT as f32 / DISPLAY as f32;

        // The count: hidden in the table until its card has been shown, then it stays.
        let digits = p.votes.to_string().len() as f32;
        let row_count_x = ROW_COUNT_RIGHT - digits * layout.cell + ride_x;
        let row_count_y = row_y + 82.0 + ride_y;
        let row_count_o = if t >= cue { row_o } else { 0.0 };
        let count = (p.votes as f32 * ease(&COUNT, t, cue + COUNT_AT)).round() as u32;

        let tagline: Vec<Svgr> = layout.taglines[i]
            .iter()
            .enumerate()
            .map(|(k, line)| {
                let (dy, o) = rise(t, cue + 0.05 + k as f32 * 0.07, 18.0);
                let y = CARD_TAGLINE_Y + k as f32 * CARD_TAGLINE_LEAD + dy;
                fframes::svgr!(<text x={MARGIN} y={y} opacity={o * card} font-family={FONT} font-size={TAGLINE} font-weight="500" fill={ink}>{line.clone()}</text>)
            })
            .collect();
        let (label_dy, label_o) = rise(t, cue + 0.1, 12.0);

        let gallery = if card > 0.0 { self.gallery(ctx, p, i, t, cue, card) } else { Svgr::empty() };

        fframes::svgr!(<g>
            <rect x={fx} y={fy} width={fw} height={fh} fill={field} />
            {bar}
            {logo(ctx, p, logo_x, logo_y, logo_size)}
            <g opacity={row_o} transform={format!("translate({ride_x} {ride_y})")}>
                <g transform={format!("translate({ROW_RANK_X} {}) scale({rank_scale})", row_y + 82.0)}>
                    <text x="0" y="0" font-family={FONT} font-size={DISPLAY} font-weight="700" letter-spacing="-2" fill={muted}>{RANK_LABELS[i]}</text>
                </g>
                <g transform={format!("translate({ROW_NAME_X} {}) scale({row_scale})", row_y + 82.0)}>
                    <text x="0" y="0" font-family={FONT} font-size={*name_size} font-weight="700" letter-spacing={-(*name_size as f32) * 0.025} fill={ink}>{name.clone()}</text>
                </g>
            </g>
            <g opacity={row_count_o}>
                {upvote(row_count_x - 38.0, row_count_y, 26.0)}
                {tabular(p.votes, row_count_x, row_count_y, layout.cell, TEXT, 500, ink)}
            </g>
            <g opacity={card_o} transform={format!("translate(0 {card_dy})")}>
                <text x={card_rank_x} y="158" font-family={FONT} font-size={DISPLAY} font-weight="700" letter-spacing="-2" fill={ink}>{RANK_LABELS[i]}</text>
                <text x={MARGIN - 3.0} y={CARD_NAME_Y} font-family={FONT} font-size={*name_size} font-weight="700" letter-spacing={-(*name_size as f32) * 0.025} fill={ink}>{name.clone()}</text>
                {upvote(CARD_COUNT_X - 38.0, 126.0, 26.0)}
                {tabular(count, CARD_COUNT_X, 126.0, layout.cell, TEXT, 500, ink)}
            </g>
            <text x={CARD_COUNT_X - 38.0} y={166.0 + label_dy} opacity={label_o * card} font-family={FONT} font-size={SMALL} font-weight="500" fill={muted}>"upvotes"</text>
            {tagline}
            {gallery}
        </g>)
    }

    /// The launch's images, square cornered, within the margins, shown whole. Each new
    /// image pushes the previous one out to the left.
    fn gallery<'b>(&'b self, ctx: &FFramesContext<'b, '_>, p: &'b Product, i: usize, t: f32, cue: f32, card: f32) -> Svgr<'b> {
        let shots: Vec<_> = p.shots.iter().filter_map(|s| ctx.get_image(s)).take(3).collect();
        let w = INNER;
        let h = SHOT_H;
        let (dy, o) = rise(t, cue - 0.1, 70.0);
        let content: Svgr = if shots.is_empty() {
            logo(ctx, p, MARGIN + w / 2.0 - 140.0, SHOT_Y + h / 2.0 - 140.0, 280.0)
        } else {
            let switches = SHOT_SWITCHES[shots.len() - 1];
            let images: Vec<Svgr> = shots
                .iter()
                .enumerate()
                .map(|(k, img)| {
                    let since = cue + switches[k];
                    let until = switches.get(k + 1).map(|s| cue + s);
                    let enter = if k == 0 { 1.0 } else { ease(&PUSH, t, since) };
                    let leave = until.map_or(0.0, |u| ease(&PUSH, t, u));
                    if (k > 0 && t < since) || leave >= 1.0 {
                        return Svgr::empty();
                    }
                    let x = MARGIN + (1.0 - enter) * w - leave * w;
                    let zoom = 1.0 + 0.04 * ((t - since) / PER_PRODUCT).clamp(0.0, 1.0);
                    let (cx, cy) = (x + w / 2.0, SHOT_Y + h / 2.0);
                    let transform = format!("translate({} {}) scale({zoom})", cx * (1.0 - zoom), cy * (1.0 - zoom));
                    fframes::svgr!(<image href={img.href()} x={x} y={SHOT_Y} width={w} height={h} transform={transform} preserveAspectRatio="xMidYMid slice" />)
                })
                .collect();
            fframes::svgr!(<g>{images}</g>)
        };
        let id = format!("shots{i}");
        fframes::svgr!(<g opacity={o * card} transform={format!("translate(0 {dy})")}>
            <defs><clipPath id={id.clone()}><rect x={MARGIN} y={SHOT_Y} width={w} height={h} /></clipPath></defs>
            <g clip-path={format!("url(#{id})")}>{content}</g>
        </g>)
    }
}

impl std::fmt::Debug for PhTodayVideo<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PhTodayVideo").field("products", &self.board.products.len()).finish()
    }
}

impl Video for PhTodayVideo<'_> {
    const FPS: usize = 60;
    const WIDTH: usize = WIDTH;
    const HEIGHT: usize = HEIGHT;
    const BACKGROUND_COLOR: Color = Color::WHITE;

    fn duration(&self) -> Duration<'_> {
        Duration::Auto
    }

    fn audio(&self) -> AudioMap<'_> {
        let n = self.board.products.len();
        // music.wav is laid out for five launches; with fewer, skip whole 6 s blocks of the
        // countdown so the #1 section and the leaderboard still land on their cues.
        let skip = (5 - n) as f32 * PER_PRODUCT;
        let first = cue_of(0, n);
        let mut tracks = vec![
            AudioTrack::new("music.wav", Second(0.)..Second(INTRO)).gain_db(-3.),
            AudioTrack::new("music.wav", Second(INTRO)..Eof).offset(INTRO + skip).gain_db(-5.),
            AudioTrack::new("impact.wav", Second(first)..Eof).gain_db(-13.),
        ];
        if n > 1 {
            tracks.push(AudioTrack::new("riser.wav", Second(first - 1.5)..Eof).gain_db(-19.));
        }
        for i in 0..n {
            let cue = cue_of(i, n);
            // whoosh.wav peaks at 0.25 s: the middle of the opening and of the fold.
            tracks.push(AudioTrack::new("whoosh.wav", Second(cue - OPEN / 2.0 - 0.25)..Eof).gain_db(-19.));
            tracks.push(AudioTrack::new("whoosh.wav", Second(cue + CLOSE_AT + OPEN / 2.0 - 0.25)..Eof).gain_db(-24.));
            // Ticks follow the counter: dense while it races, sparse as it settles.
            let ticks = 10;
            for k in 1..=ticks {
                let target = k as f32 / (ticks + 1) as f32;
                let (mut lo, mut hi) = (0.0, COUNT.get_duration());
                for _ in 0..24 {
                    let mid = (lo + hi) / 2.0;
                    if COUNT.solve(&mid) < target { lo = mid } else { hi = mid }
                }
                tracks.push(AudioTrack::new("tick.wav", Second(cue + COUNT_AT + hi)..Eof).gain_db(-27. - k as f32 * 0.6));
            }
        }
        AudioMap::from(tracks)
    }

    fn define_scenes(&self) -> Scenes<'_> {
        Scenes::from(self.parts.iter().map(|p| p as &dyn Scene).collect::<Vec<_>>())
    }

    fn render_frame<'a>(&'a self, mut frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let t = frame.seconds();
        let layout = self.layout(&mut frame, ctx);
        let n = self.board.products.len();

        // Already on screen in the first frame (feeds autoplay), still settling.
        let (h_dy, h_o) = rise(t, -0.25, 24.0);
        let (d_dy, d_o) = rise(t, -0.1, 24.0);

        // The table, rows rising in one after another. An open card is drawn last, over the
        // rest, so it covers the table while it grows.
        let open_index = (0..n).find(|&i| {
            let cue = cue_of(i, n);
            t >= cue - OPEN && t < cue + CLOSE_AT + OPEN
        });
        let mut rows: Vec<Svgr> = (0..n)
            .filter(|&i| Some(i) != open_index)
            .map(|i| {
                let (dy, o) = rise(t, 0.1 + i as f32 * 0.08, 28.0);
                fframes::svgr!(<g opacity={o} transform={format!("translate(0 {dy})")}>{self.product(ctx, layout, i, t)}</g>)
            })
            .collect();
        if let Some(i) = open_index {
            rows.push(self.product(ctx, layout, i, t));
        }

        // The last half second settles to white.
        let fade = 1.0 - ease(&OUT, t, self.board.total() - 0.6);

        fframes::svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" width={WIDTH} height={HEIGHT} viewBox="0 0 1080 1080">
                <rect width="1080" height="1080" fill={GROUND} />
                <g opacity={fade}>
                    <text x={MARGIN - 4.0} y={160.0 + h_dy} opacity={h_o} font-family={FONT} font-size={DISPLAY} font-weight="700" letter-spacing="-2.4" fill={INK}>{layout.headline.clone()}</text>
                    <text x={MARGIN} y={224.0 + d_dy} opacity={d_o} font-family={FONT} font-size={TEXT} font-weight="500" fill={MUTED}>{self.board.date_label.clone()}</text>
                    {rows}
                </g>
            </svg>
        )
    }
}
