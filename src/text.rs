use crate::color::Color;
use pango::{EllipsizeMode, FontDescription};
use pangocairo::{cairo, pango};
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq)]
pub struct RenderOptions {
    pub x_offset: f64,
    pub bar_height: f64,
    pub fg_color: Color,
    pub bg_color: Option<Color>,
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

fn create_pango_layout(cairo_context: &cairo::Context) -> pango::Layout {
    pangocairo::create_layout(cairo_context).unwrap()
}

fn show_pango_layout(cairo_context: &cairo::Context, layout: &pango::Layout) {
    pangocairo::show_layout(cairo_context, layout);
}

#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    pub attr: Attributes,
    pub text: String,
}

impl Text {
    pub fn compute(mut self, context: &cairo::Context) -> ComputedText {
        let (mut width, height) = {
            let layout = create_pango_layout(context);
            layout.set_font_description(Some(&self.attr.font));
            if self.attr.markup {
                layout.set_markup(&self.text);
            } else {
                layout.set_text(&self.text);
            }

            let (text_width, text_height) = layout.pixel_size();
            let width = f64::from(text_width) + self.attr.padding_right + self.attr.padding_right;
            let height = f64::from(text_height);
            (width, height)
        };

        if let Some(min_width) = self.attr.min_width {
            if width < min_width {
                let d = min_width - width;
                width = min_width;
                match self.attr.align {
                    Align::Right => self.attr.padding_left += d,
                    Align::Left => self.attr.padding_right += d,
                    Align::Center => {
                        self.attr.padding_left += d * 0.5;
                        self.attr.padding_right += d * 0.5;
                    }
                }
            }
        }

        ComputedText {
            text: self,
            width,
            height,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ComputedText {
    pub text: Text,
    pub width: f64,
    pub height: f64,
}

impl ComputedText {
    pub fn render(&self, context: &cairo::Context, options: RenderOptions) {
        let text = &self.text;
        let layout = create_pango_layout(context);
        layout.set_font_description(Some(&text.attr.font));
        if text.attr.markup {
            layout.set_markup(&text.text);
        } else {
            layout.set_text(&text.text);
        }

        context.save().unwrap();
        context.translate(options.x_offset, 0.0);

        // Set the width/height on the Pango layout so that it word-wraps/ellipises.
        let text_width = self.width - text.attr.padding_left - text.attr.padding_right;
        let text_height = self.height;
        layout.set_ellipsize(EllipsizeMode::End);
        layout.set_width(text_width as i32 * pango::SCALE);
        layout.set_height(text_height as i32 * pango::SCALE);

        if let Some(bg) = options.bg_color {
            bg.apply(&context);
            context.rectangle(0.0, 0.0, self.width, options.bar_height);
            context.fill().unwrap();
        }

        options.fg_color.apply(&context);
        context.translate(
            text.attr.padding_left,
            (options.bar_height - self.height) * 0.5,
        );
        show_pango_layout(context, &layout);
        context.restore().unwrap();
    }
}

pub fn width_of(text: &str, context: &cairo::Context, markup: bool, font: &FontDescription) -> f64 {
    let text = Text {
        text: text.into(),
        attr: Attributes {
            font: font.clone(),
            padding_left: 0.0,
            padding_right: 0.0,
            min_width: None,
            align: Default::default(),
            markup,
        },
    };
    text.compute(context).width
}
