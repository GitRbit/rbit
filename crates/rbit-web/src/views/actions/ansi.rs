//! ANSI escapes of a log line as theme classes, never as literal colours.
//!
//! Select Graphic Rendition (`ESC [ … m`) sets the 16 standard colours, their background
//! forms, bold, dim, italic and underline; 256-colour and true-colour values are folded to
//! the nearest of the 16. Each colour is a class (`ansi-f1` red text, `ansi-b4` blue
//! background …) that app.css maps to the theme's tokens, so tinted themes recolour logs too.
//! Every other escape (cursor moves, OSC titles and links) is dropped. A carriage return
//! starts the line over, as a terminal draws progress bars, and other control characters
//! but tab are left out. Text is escaped; the style starts afresh on every line.

/// The style the next text is drawn in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[expect(clippy::struct_excessive_bools, reason = "the independent SGR attributes")]
struct Style {
    fg: Option<u8>,
    bg: Option<u8>,
    bold: bool,
    dim: bool,
    italic: bool,
    underline: bool,
    inverse: bool,
}

impl Style {
    fn classes(self) -> String {
        let (fg, bg) = if self.inverse { (self.bg, self.fg) } else { (self.fg, self.bg) };
        let mut classes = Vec::new();
        if let Some(fg) = fg {
            classes.push(format!("ansi-f{fg}"));
        }
        if let Some(bg) = bg {
            classes.push(format!("ansi-b{bg}"));
        }
        if self.inverse && self.fg.is_none() && self.bg.is_none() {
            classes.push("ansi-inverse".to_owned());
        }
        for (on, class) in
            [(self.bold, "ansi-bold"), (self.dim, "ansi-dim"), (self.italic, "ansi-italic"), (self.underline, "ansi-underline")]
        {
            if on {
                classes.push(class.to_owned());
            }
        }
        classes.join(" ")
    }

    /// Applies one SGR parameter list (`1;31`, `38;5;208`, `38:2::255:0:0`).
    fn apply(&mut self, params: &str) {
        let mut values = params.split([';', ':']).map(|value| value.parse::<u16>().unwrap_or(0)).peekable();
        if params.is_empty() {
            *self = Self::default();
            return;
        }
        while let Some(code) = values.next() {
            match code {
                0 => *self = Self::default(),
                1 => self.bold = true,
                2 => self.dim = true,
                3 => self.italic = true,
                4 => self.underline = true,
                7 => self.inverse = true,
                21 | 22 => (self.bold, self.dim) = (false, false),
                23 => self.italic = false,
                24 => self.underline = false,
                27 => self.inverse = false,
                30..=37 => self.fg = Some(small(code - 30)),
                90..=97 => self.fg = Some(small(code - 90 + 8)),
                40..=47 => self.bg = Some(small(code - 40)),
                100..=107 => self.bg = Some(small(code - 100 + 8)),
                39 => self.fg = None,
                49 => self.bg = None,
                38 | 48 => {
                    let colour = match values.next() {
                        Some(5) => values.next().map(indexed),
                        Some(2) => {
                            // `38:2::r:g:b` carries an empty colour-space id before the channels.
                            let colon = params.contains(':');
                            let mut channels: Vec<u16> = values.by_ref().take(if colon { 4 } else { 3 }).collect();
                            if channels.len() == 4 {
                                channels.remove(0);
                            }
                            (channels.len() == 3).then(|| nearest(channels[0], channels[1], channels[2]))
                        }
                        _ => None,
                    };
                    if code == 38 { self.fg = colour } else { self.bg = colour }
                }
                _ => {}
            }
        }
    }
}

fn small(value: u16) -> u8 {
    u8::try_from(value).unwrap_or(0)
}

/// One of the 256 xterm colours folded to the 16 standard ones.
fn indexed(index: u16) -> u8 {
    match index {
        0..=15 => small(index),
        16..=231 => {
            let cube = index - 16;
            let level = |value: u16| if value == 0 { 0 } else { 55 + value * 40 };
            nearest(level(cube / 36), level((cube / 6) % 6), level(cube % 6))
        }
        _ => {
            let gray = 8 + (index.min(255) - 232) * 10;
            nearest(gray, gray, gray)
        }
    }
}

/// The standard colour nearest `r`, `g`, `b` (0–255): grey shades by brightness, other
/// colours by which channels stand out.
fn nearest(r: u16, g: u16, b: u16) -> u8 {
    let (r, g, b) = (r.min(255), g.min(255), b.min(255));
    let high = r.max(g).max(b);
    let low = r.min(g).min(b);
    if high - low < 40 {
        return match high {
            0..=63 => 0,
            64..=159 => 8,
            160..=223 => 7,
            _ => 15,
        };
    }
    let threshold = low + (high - low) / 2;
    let bits = u8::from(r > threshold) | u8::from(g > threshold) << 1 | u8::from(b > threshold) << 2;
    if high > 200 { bits + 8 } else { bits }
}

/// Appends `text` escaped, without control characters but tab.
fn push_text(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            '\t' => out.push('\t'),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
}

/// `line` as escaped HTML with its colours as `ansi-*` classes, appended to `out`.
pub fn ansi_html(line: &str, out: &mut String) {
    let start = out.len();
    let mut style = Style::default();
    let mut open = false;
    let mut chars = line.char_indices().peekable();
    let mut text_from = 0;
    let flush = |out: &mut String, open: &mut bool, style: Style, text: &str| {
        if text.is_empty() {
            return;
        }
        if !*open && style != Style::default() {
            out.push_str(r#"<span class=""#);
            out.push_str(&style.classes());
            out.push_str(r#"">"#);
            *open = true;
        }
        push_text(out, text);
    };
    while let Some((at, c)) = chars.next() {
        if c != '\u{1b}' && c != '\r' {
            continue;
        }
        flush(out, &mut open, style, &line[text_from..at]);
        if c == '\r' {
            // A terminal goes back to the start of the line: what follows overwrites it.
            if chars.peek().is_some() {
                out.truncate(start);
                open = false;
            }
            text_from = at + 1;
            continue;
        }
        let mut end = at + 1;
        match chars.peek().map(|&(_, next)| next) {
            Some('[') => {
                chars.next();
                let params_from = at + 2;
                let mut final_byte = None;
                for (index, c) in chars.by_ref() {
                    end = index + c.len_utf8();
                    if ('@'..='~').contains(&c) {
                        final_byte = Some((index, c));
                        break;
                    }
                }
                if let Some((index, 'm')) = final_byte {
                    let before = style;
                    style.apply(&line[params_from..index]);
                    if style != before && open {
                        out.push_str("</span>");
                        open = false;
                    }
                }
            }
            Some(']') => {
                chars.next();
                while let Some((index, c)) = chars.next() {
                    end = index + c.len_utf8();
                    if c == '\u{7}' {
                        break;
                    }
                    if c == '\u{1b}' && chars.next_if(|&(_, next)| next == '\\').is_some() {
                        end = index + 2;
                        break;
                    }
                }
            }
            Some(_) => {
                if let Some((index, c)) = chars.next() {
                    end = index + c.len_utf8();
                }
            }
            None => {}
        }
        text_from = end;
    }
    flush(out, &mut open, style, &line[text_from..]);
    if open {
        out.push_str("</span>");
    }
}

/// `line` without any escape or control character but tab: its plain text.
pub fn plain(line: &str) -> String {
    let mut html = String::with_capacity(line.len());
    ansi_html(line, &mut html);
    let mut text = String::with_capacity(html.len());
    let mut rest = html.as_str();
    while let Some(open) = rest.find('<') {
        text.push_str(&rest[..open]);
        rest = rest[open..].split_once('>').map_or("", |(_, after)| after);
    }
    text.push_str(rest);
    text.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn html(line: &str) -> String {
        let mut out = String::new();
        ansi_html(line, &mut out);
        out
    }

    #[test]
    fn colours_become_theme_classes_and_text_is_escaped() {
        assert_eq!(html("plain <b>&"), "plain &lt;b&gt;&amp;");
        assert_eq!(html("\u{1b}[32mok\u{1b}[0m done"), r#"<span class="ansi-f2">ok</span> done"#);
        assert_eq!(html("\u{1b}[1;31mE\u{1b}[22mrr"), r#"<span class="ansi-f1 ansi-bold">E</span><span class="ansi-f1">rr</span>"#);
        assert_eq!(html("\u{1b}[93;44mx"), r#"<span class="ansi-f11 ansi-b4">x</span>"#);
        assert_eq!(html("\u{1b}[38;5;196mred\u{1b}[39m"), r#"<span class="ansi-f9">red</span>"#);
        assert_eq!(html("\u{1b}[38;2;0;200;0mgreen"), r#"<span class="ansi-f2">green</span>"#);
        assert_eq!(html("\u{1b}[38:2::0:0:255mblue"), r#"<span class="ansi-f12">blue</span>"#);
        assert_eq!(html("\u{1b}[38;5;244mgrey"), r#"<span class="ansi-f8">grey</span>"#);
        assert_eq!(html("\u{1b}[7mrev"), r#"<span class="ansi-inverse">rev</span>"#);
        assert_eq!(html("\u{1b}[31;7mrev"), r#"<span class="ansi-b1">rev</span>"#);
        assert_eq!(html("\u{1b}[2;3;4mstyled\u{1b}[m"), r#"<span class="ansi-dim ansi-italic ansi-underline">styled</span>"#);
        assert!(!html("\u{1b}[31m<script>").contains("<script>"));
    }

    #[test]
    fn other_escapes_and_controls_are_dropped() {
        assert_eq!(html("\u{1b}]8;;https://x\u{7}link\u{1b}]8;;\u{1b}\\ done"), "link done");
        assert_eq!(html("a\u{1b}[2Kb\u{1b}[1Gc"), "abc");
        assert_eq!(html("bell\u{7}\u{0}\tok"), "bell\tok");
        assert_eq!(html("plain \u{1b}"), "plain ");
        assert_eq!(html("\u{1b}[31"), "", "an unfinished escape is left out");
        assert_eq!(html("10%\r50%\r\u{1b}[32m100%"), r#"<span class="ansi-f2">100%</span>"#, "progress redraws");
        assert_eq!(html("done\r"), "done", "a trailing return changes nothing");
    }

    #[test]
    fn plain_text_has_no_markup() {
        assert_eq!(plain("\u{1b}[31m<it> & \"broke\"\u{1b}[0m"), "<it> & \"broke\"");
    }
}
