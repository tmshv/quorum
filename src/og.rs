//! Link-preview cards: a 1200×630 PNG with the room's name, drawn on the
//! server because link-preview crawlers do not run JavaScript.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};
use tiny_skia::{
    Color, FillRule, Paint, PathBuilder, Pixmap, PremultipliedColorU8, Rect, Stroke, Transform,
};
use tokio::sync::Semaphore;

pub const WIDTH: u32 = 1200;
pub const HEIGHT: u32 = 630;

/// Rendered cards kept in memory; the whole cache is dropped when full.
const CACHE_CAP: usize = 256;
/// Renders allowed at once; more are refused rather than queued.
const RENDERS: usize = 2;

static BOLD: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf");
static REGULAR: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf");

/// Same colours as the front end: Cool Glow chrome and the people palette.
const BG: [u8; 3] = [0x06, 0x05, 0x21];
const PANEL: [u8; 3] = [0x0c, 0x0b, 0x31];
const BORDER: [u8; 3] = [0x12, 0x2b, 0xbb];
const FG: [u8; 3] = [0xe0, 0xe0, 0xe0];
const MUTED: [u8; 3] = [0x80, 0x80, 0x98];
const FLAG_TEXT: [u8; 3] = [0x15, 0x13, 0x1f];
const PALETTE: [[u8; 3]; 8] = [
    [0xff, 0xcf, 0x5a],
    [0xff, 0x7b, 0x6b],
    [0x7e, 0xe0, 0xa1],
    [0xc3, 0xa6, 0xff],
    [0xff, 0x9f, 0xd1],
    [0x7f, 0xd1, 0xff],
    [0xff, 0xb3, 0x6b],
    [0x9b, 0xe7, 0xe0],
];
/// Who is "already typing" in the card: picked from the room name, so a room
/// always gets the same one.
const TYPISTS: [&str; 8] = [
    "Kernel Panic",
    "Race Condition",
    "Rubber Duck",
    "Heisenbug",
    "Off By One",
    "Null Pointer",
    "Deadlock",
    "Segfault",
];

/// Draws the card for `room`, or the generic card for the front page.
pub fn render(room: Option<&str>) -> Vec<u8> {
    let bold = FontRef::try_from_slice(BOLD).expect("embedded font parses");
    let regular = FontRef::try_from_slice(REGULAR).expect("embedded font parses");
    let mut pm = Pixmap::new(WIDTH, HEIGHT).expect("non-zero size");
    pm.fill(color(BG));

    // Editor panel.
    let panel = rounded(48.0, 48.0, 1104.0, 486.0, 22.0);
    pm.fill_path(
        &panel,
        &paint(PANEL),
        FillRule::Winding,
        Transform::identity(),
        None,
    );
    let stroke = Stroke {
        width: 2.0,
        ..Stroke::default()
    };
    pm.stroke_path(&panel, &paint(BORDER), &stroke, Transform::identity(), None);

    // Title bar: wordmark and window dots.
    let x0 = 96.0;
    let w = text(&mut pm, &bold, 34.0, x0, 118.0, FG, "quorum");
    if room.is_some() {
        text(&mut pm, &regular, 34.0, x0 + w, 118.0, MUTED, " / room");
    }
    for (i, c) in [PALETTE[1], PALETTE[0], PALETTE[2]].iter().enumerate() {
        let path = PathBuilder::from_circle(1040.0 + i as f32 * 30.0, 106.0, 8.0).expect("circle");
        pm.fill_path(
            &path,
            &paint(*c),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
    fill_rect(&mut pm, 49.0, 152.0, 1102.0, 2.0, BORDER);

    // Line 1: the room name, as big as fits.
    let name = room.unwrap_or("quorum");
    let gutter = x0 + 64.0;
    let avail = 1104.0 + 48.0 - 64.0 - gutter;
    let size = (avail / (name.chars().count() as f32 * advance_em(&bold) + 1.0)).clamp(30.0, 96.0);
    let base1 = 320.0;
    text(
        &mut pm,
        &regular,
        26.0,
        x0,
        base1 - (size - 26.0) * 0.35,
        MUTED,
        "1",
    );
    let name_w = text(&mut pm, &bold, size, gutter, base1, FG, name);

    // Someone's cursor at the end of the name, flag always up.
    let seed = room.map_or(0, hash);
    let who = TYPISTS[seed % TYPISTS.len()];
    let tint = PALETTE[(seed / TYPISTS.len()) % PALETTE.len()];
    let caret_x = gutter + name_w + 4.0;
    let caret_top = base1 - size * 0.86;
    fill_rect(&mut pm, caret_x, caret_top, 4.0, size * 1.06, tint);
    let flag_size = 26.0;
    let flag_w = advance_em(&bold) * flag_size * who.chars().count() as f32 + 24.0;
    let flag_h = 38.0;
    let flag_y = caret_top - flag_h;
    let flag_x = if caret_x + flag_w > 1130.0 {
        caret_x + 4.0 - flag_w
    } else {
        caret_x
    };
    let flag = rounded(flag_x, flag_y, flag_w, flag_h, 7.0);
    pm.fill_path(
        &flag,
        &paint(tint),
        FillRule::Winding,
        Transform::identity(),
        None,
    );
    text(
        &mut pm,
        &bold,
        flag_size,
        flag_x + 12.0,
        flag_y + 28.0,
        FLAG_TEXT,
        who,
    );

    // Lines 2–3: what this is.
    let base2 = 420.0;
    text(&mut pm, &regular, 26.0, x0, base2, MUTED, "2");
    text(
        &mut pm,
        &regular,
        30.0,
        gutter,
        base2,
        MUTED,
        "// a shared code pad for interviews",
    );
    text(&mut pm, &regular, 26.0, x0, base2 + 56.0, MUTED, "3");
    let line3 = if room.is_some() {
        "// open the link, type together"
    } else {
        "// open a room, send the link"
    };
    text(&mut pm, &regular, 30.0, gutter, base2 + 56.0, MUTED, line3);

    // The palette along the bottom edge.
    let seg = WIDTH as f32 / PALETTE.len() as f32;
    for (i, c) in PALETTE.iter().enumerate() {
        fill_rect(
            &mut pm,
            i as f32 * seg,
            HEIGHT as f32 - 14.0,
            seg + 1.0,
            14.0,
            *c,
        );
    }

    pm.encode_png().expect("png encodes")
}

/// Renders behind a small cache and a cap on concurrent renders.
pub struct Cards {
    cache: Mutex<HashMap<String, Arc<[u8]>>>,
    gate: Arc<Semaphore>,
}

impl Default for Cards {
    fn default() -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
            gate: Arc::new(Semaphore::new(RENDERS)),
        }
    }
}

impl Cards {
    /// The card for `room` (None: the front page), or None when busy.
    pub async fn get(&self, room: Option<&str>) -> Option<Arc<[u8]>> {
        let key = room.unwrap_or("").to_owned();
        if let Some(png) = self.cache.lock().expect("cache lock").get(&key) {
            return Some(png.clone());
        }
        let permit = self.gate.clone().try_acquire_owned().ok()?;
        let owned = room.map(str::to_owned);
        let png: Arc<[u8]> = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            render(owned.as_deref())
        })
        .await
        .ok()?
        .into();
        let mut cache = self.cache.lock().expect("cache lock");
        if cache.len() >= CACHE_CAP {
            cache.clear();
        }
        cache.insert(key, png.clone());
        Some(png)
    }
}

fn hash(s: &str) -> usize {
    // FNV-1a: stable across runs and platforms, unlike the std hasher.
    let mut h: u32 = 0x811c_9dc5;
    for b in s.bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    h as usize
}

fn color([r, g, b]: [u8; 3]) -> Color {
    Color::from_rgba8(r, g, b, 255)
}

fn paint(c: [u8; 3]) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(color(c));
    p.anti_alias = true;
    p
}

fn fill_rect(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, c: [u8; 3]) {
    if let Some(r) = Rect::from_xywh(x, y, w, h) {
        pm.fill_rect(r, &paint(c), Transform::identity(), None);
    }
}

fn rounded(x: f32, y: f32, w: f32, h: f32, r: f32) -> tiny_skia::Path {
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish().expect("non-empty path")
}

/// Horizontal advance of one character, in ems (the font is monospaced).
fn advance_em(font: &FontRef<'_>) -> f32 {
    let scaled = font.as_scaled(PxScale::from(1.0));
    scaled.h_advance(font.glyph_id('M'))
}

/// Draws `s` with its baseline at `y` over an opaque pixmap; returns the width.
fn text(pm: &mut Pixmap, font: &FontRef<'_>, px: f32, x: f32, y: f32, c: [u8; 3], s: &str) -> f32 {
    let scaled = font.as_scaled(PxScale::from(px));
    let (w, h) = (pm.width() as i32, pm.height() as i32);
    let pixels = pm.pixels_mut();
    let mut cx = x;
    for ch in s.chars() {
        let id = scaled.glyph_id(ch);
        let glyph = id.with_scale_and_position(px, point(cx, y));
        if let Some(outline) = scaled.outline_glyph(glyph) {
            let b = outline.px_bounds();
            outline.draw(|gx, gy, cov| {
                let px_x = b.min.x as i32 + gx as i32;
                let px_y = b.min.y as i32 + gy as i32;
                if px_x < 0 || px_y < 0 || px_x >= w || px_y >= h {
                    return;
                }
                let i = (px_y * w + px_x) as usize;
                let dst = pixels[i];
                let a = cov.clamp(0.0, 1.0);
                let mix =
                    |s: u8, d: u8| (f32::from(s) * a + f32::from(d) * (1.0 - a)).round() as u8;
                pixels[i] = PremultipliedColorU8::from_rgba(
                    mix(c[0], dst.red()),
                    mix(c[1], dst.green()),
                    mix(c[2], dst.blue()),
                    255,
                )
                .expect("opaque colour is valid");
            });
        }
        cx += scaled.h_advance(id);
    }
    cx - x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_a_png_of_the_card_size() {
        let png = render(Some("kernel-panic-42"));
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let pm = Pixmap::decode_png(&png).unwrap();
        assert_eq!((pm.width(), pm.height()), (WIDTH, HEIGHT));
    }

    #[test]
    fn the_name_changes_the_picture() {
        assert_ne!(render(Some("abc")), render(Some("xyz")));
        assert_eq!(render(Some("abc")), render(Some("abc")));
    }

    #[test]
    fn longest_room_name_renders() {
        let long = "a".repeat(40);
        assert!(!render(Some(&long)).is_empty());
        assert!(!render(None).is_empty());
    }

    #[tokio::test]
    async fn cards_are_cached() {
        let cards = Cards::default();
        let a = cards.get(Some("room-1")).await.unwrap();
        let b = cards.get(Some("room-1")).await.unwrap();
        assert!(Arc::ptr_eq(&a, &b));
    }
}
