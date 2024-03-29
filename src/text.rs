use crate::color::Color;
use pango::FontDescription;
use pangocairo::{cairo, pango};
use serde::Deserialize;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

thread_local! {
    pub static PANGO_CTX: pango::Context = {
        let context = pango::Context::new();
        let fontmap = pangocairo::FontMap::new();
        context.set_font_map(Some(&fontmap));
        context
    };
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderOptions {
    pub x_offset: f64,
    pub bar_height: f64,
    pub fg_color: Color,
    pub bg_color: Option<Color>,
    pub r_left: f64,
    pub r_right: f64,
    pub overlap: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Attributes<'a> {
    pub font: &'a FontDescription,
    pub padding_left: f64,
    pub padding_right: f64,
    pub min_width: Option<f64>,
    pub align: Align,
    pub markup: bool,
}

#[derive(Deserialize, Default, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    Right,
    #[default]
    Left,
    Center,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ComputedText {
    pub width: f64,
    layout: pango::Layout,
    height: f64,
    padding_left: f64,
}

impl ComputedText {
    pub fn new(text: &str, mut attr: Attributes) -> Self {
        let text = text.replace('\n', "\u{23CE}");

        let layout = PANGO_CTX.with(pango::Layout::new);
        layout.set_font_description(Some(attr.font));
        if attr.markup {
            layout.set_markup(&text);
        } else {
            layout.set_text(&text);
        }

        let (text_width, text_height) = layout.pixel_size();
        let mut width = f64::from(text_width) + attr.padding_right + attr.padding_right;
        let height = f64::from(text_height);

        if let Some(min_width) = attr.min_width {
            if width < min_width {
                let d = min_width - width;
                width = min_width;
                match attr.align {
                    Align::Right => attr.padding_left += d,
                    Align::Left => attr.padding_right += d,
                    Align::Center => {
                        attr.padding_left += d * 0.5;
                        attr.padding_right += d * 0.5;
                    }
                }
            }
        }

        Self {
            width,
            layout,
            height,
            padding_left: attr.padding_left,
        }
    }

    pub fn render(&self, context: &cairo::Context, options: RenderOptions) {
        context.save().unwrap();
        context.translate(options.x_offset - options.overlap, 0.0);

        // Draw background
        if let Some(bg) = options.bg_color {
            bg.apply(context);
            rounded_rectangle(
                context,
                0.0,
                0.0,
                // HACK: this `+ 0.5` fixes some artifacts of fractional scaling
                self.width + options.overlap + 0.5,
                options.bar_height,
                options.r_left,
                options.r_right,
            );
            context.fill().unwrap();
        }

        options.fg_color.apply(context);
        context.translate(
            self.padding_left + options.overlap,
            (options.bar_height - self.height) * 0.5,
        );
        pangocairo::functions::show_layout(context, &self.layout);
        context.restore().unwrap();
    }
}

fn rounded_rectangle(
    context: &cairo::Context,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    r_left: f64,
    r_right: f64,
) {
    if r_left > 0.0 || r_right > 0.0 {
        context.new_sub_path();
        context.arc(x + r_left, y + r_left, r_left, PI, 3.0 * FRAC_PI_2);
        context.arc(x + w - r_right, y + r_right, r_right, 3.0 * FRAC_PI_2, TAU);
        context.arc(x + w - r_right, y + h - r_right, r_right, 0.0, FRAC_PI_2);
        context.arc(x + r_left, y + h - r_left, r_left, FRAC_PI_2, PI);
        context.close_path();
    } else {
        context.rectangle(x, y, w, h);
    }
}

pub fn width_of(text: &str, markup: bool, font: &FontDescription) -> f64 {
    ComputedText::new(
        text,
        Attributes {
            font,
            padding_left: 0.0,
            padding_right: 0.0,
            min_width: None,
            align: Default::default(),
            markup,
        },
    )
    .width
}
