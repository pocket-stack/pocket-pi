use pocketjs_core::{spec, Ui};

const BODY_SLOT: u8 = 3;
const DISPLAY_SLOT: u8 = 6;
const BODY_BOLD_SLOT: u8 = 10;
const TITLE_SLOT: u8 = 12;
const COLUMN_WIDTH: usize = 16;

const BODY_ATLAS: &[u8] = include_bytes!("../../assets/fonts/inter-18-regular.pfa");
const DISPLAY_ATLAS: &[u8] = include_bytes!("../../assets/fonts/inter-36-regular.pfa");
const BODY_BOLD_ATLAS: &[u8] = include_bytes!("../../assets/fonts/inter-18-bold.pfa");
const TITLE_ATLAS: &[u8] = include_bytes!("../../assets/fonts/inter-24-bold.pfa");

#[derive(Clone, Copy)]
pub enum TextStyle {
    Body,
    Bold,
    Title,
    Display,
}

impl TextStyle {
    const fn slot(self) -> u8 {
        match self {
            Self::Body => BODY_SLOT,
            Self::Display => DISPLAY_SLOT,
            Self::Bold => BODY_BOLD_SLOT,
            Self::Title => TITLE_SLOT,
        }
    }
}

pub fn load(ui: &mut Ui) -> bool {
    [BODY_ATLAS, DISPLAY_ATLAS, BODY_BOLD_ATLAS, TITLE_ATLAS]
        .into_iter()
        .all(|atlas| ui.load_font_atlas(atlas))
}

pub fn text_width(ui: &Ui, text: &str, style: TextStyle) -> i16 {
    let Some(atlas) = ui.font_atlas(style.slot()) else {
        return 0;
    };
    let fallback = atlas.lookup_entry('?' as u32);
    text.chars()
        .filter_map(|character| atlas.lookup_entry(character as u32).or(fallback))
        .map(|entry| i32::from(entry.advance))
        .sum::<i32>()
        .min(i32::from(i16::MAX)) as i16
}

#[allow(clippy::too_many_arguments)]
pub fn append_text(
    ui: &Ui,
    words: &mut Vec<u32>,
    text: &str,
    x: i16,
    y: i16,
    max_columns: usize,
    max_rows: usize,
    color: u32,
    style: TextStyle,
) {
    let slot = style.slot();
    let Some(atlas) = ui.font_atlas(slot) else {
        return;
    };
    let max_width = max_columns.saturating_mul(COLUMN_WIDTH) as i32;
    let line_height = atlas.line_height as i32;
    let fallback = atlas.lookup_entry('?' as u32);
    let mut glyphs = Vec::new();
    let mut pen_x = 0i32;
    let mut row = 0usize;

    for character in text.chars() {
        if character == '\n' {
            row += 1;
            pen_x = 0;
            if row >= max_rows {
                break;
            }
            continue;
        }

        let entry = atlas.lookup_entry(character as u32).or(fallback);
        let Some(entry) = entry else {
            continue;
        };
        let advance = entry.advance as i32;
        if pen_x > 0 && pen_x + advance > max_width {
            row += 1;
            pen_x = 0;
        }
        if row >= max_rows {
            break;
        }

        let glyph_x = x as i32 + pen_x - entry.xoff as i32;
        let glyph_y = y as i32 + row as i32 * line_height;
        glyphs.push(xy(glyph_x as i16, glyph_y as i16));
        glyphs.push(entry.gid as u32);
        pen_x += advance;
    }

    if glyphs.is_empty() {
        return;
    }
    words.push(spec::draw_op::GLYPH_RUN);
    words.push(slot as u32 | ((glyphs.len() as u32 / 2) << 16));
    words.push(color);
    words.extend(glyphs);
}

/// Wrap text with the same glyph advances used by `append_text`, so callers
/// can page through long content without guessing from byte or character count.
pub fn wrap_text(ui: &Ui, text: &str, max_columns: usize, style: TextStyle) -> Vec<String> {
    let Some(atlas) = ui.font_atlas(style.slot()) else {
        return vec![text.to_owned()];
    };
    let max_width = max_columns.saturating_mul(COLUMN_WIDTH) as i32;
    let fallback = atlas.lookup_entry('?' as u32);
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut pen_x = 0i32;

    for character in text.chars() {
        if character == '\n' {
            lines.push(core::mem::take(&mut line));
            pen_x = 0;
            continue;
        }
        let Some(entry) = atlas.lookup_entry(character as u32).or(fallback) else {
            continue;
        };
        let advance = entry.advance as i32;
        if pen_x > 0 && pen_x + advance > max_width {
            lines.push(core::mem::take(&mut line));
            pen_x = 0;
        }
        line.push(character);
        pen_x += advance;
    }
    lines.push(line);
    lines
}

const fn xy(x: i16, y: i16) -> u32 {
    x as u16 as u32 | ((y as u16 as u32) << 16)
}
