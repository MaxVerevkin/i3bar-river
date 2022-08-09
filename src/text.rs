use crate::color::Color;
use pango::FontDescription;
use pangocairo::{cairo, pango};
use serde::Deserialize;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

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
pub struct Attributes {
    pub font: FontDescription,
    pub padding_left: f64,
    pub padding_right: f64,
    pub min_width: Option<f64>,
    pub align: Align,
    pub markup: bool,
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    Right,
    Left,
    Center,
}

impl Default for Align {
    fn default() -> Self {
        Self::Left
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ComputedText {
    pub layout: pango::Layout,
    pub attr: Attributes,
    pub width: f64,
    pub height: f64,
}

impl ComputedText {
    pub fn new(text: &str, mut attr: Attributes, context: &cairo::Context) -> Self {
        let layout = pangocairo::create_layout(context).unwrap();
        layout.set_font_description(Some(&attr.font));
        if attr.markup {
            layout.set_markup(text);
        } else {
            layout.set_text(text);
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
            layout,
            attr,
            width,
            height,
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
                self.width + options.overlap,
                options.bar_height,
                options.r_left,
                options.r_right,
            );
            context.fill().unwrap();
        }

        options.fg_color.apply(context);
        context.translate(
            self.attr.padding_left + options.overlap,
            (options.bar_height - self.height) * 0.5,
        );
        pangocairo::show_layout(context, &self.layout);
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

pub fn width_of(text: &str, context: &cairo::Context, markup: bool, font: &FontDescription) -> f64 {
    ComputedText::new(
        text,
        Attributes {
            font: font.clone(),
            padding_left: 0.0,
            padding_right: 0.0,
            min_width: None,
            align: Default::default(),
            markup,
        },
        context,
    )
    .width
}
