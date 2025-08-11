//! Conversion functions from Stylo types to Parley types
use std::borrow::Cow;

use style::values::computed::{Length, TextDecorationLine, CSSPixelLength};

use crate::node::TextBrush;
use crate::util::ToColorColor;

// Module of type aliases so we can refer to stylo types with nicer names
pub(crate) mod stylo {
    pub(crate) use style::computed_values::white_space_collapse::T as WhiteSpaceCollapse;
    pub(crate) use style::properties::ComputedValues;
    pub(crate) use style::values::computed::OverflowWrap;
    pub(crate) use style::values::computed::WordBreak;
    pub(crate) use style::values::computed::font::FontStretch;
    pub(crate) use style::values::computed::font::FontStyle;
    pub(crate) use style::values::computed::font::FontVariationSettings;
    pub(crate) use style::values::computed::font::FontWeight;
    pub(crate) use style::values::computed::font::GenericFontFamily;
    pub(crate) use style::values::computed::font::LineHeight;
    pub(crate) use style::values::computed::font::SingleFontFamily;
}

pub(crate) mod parley {
    pub(crate) use parley::FontVariation;
    pub(crate) use parley::fontique::QueryFamily;
    pub(crate) use parley::style::*;
}

pub(crate) fn generic_font_family(input: stylo::GenericFontFamily) -> parley::GenericFamily {
    match input {
        stylo::GenericFontFamily::None => parley::GenericFamily::SansSerif,
        stylo::GenericFontFamily::Serif => parley::GenericFamily::Serif,
        stylo::GenericFontFamily::SansSerif => parley::GenericFamily::SansSerif,
        stylo::GenericFontFamily::Monospace => parley::GenericFamily::Monospace,
        stylo::GenericFontFamily::Cursive => parley::GenericFamily::Cursive,
        stylo::GenericFontFamily::Fantasy => parley::GenericFamily::Fantasy,
        stylo::GenericFontFamily::SystemUi => parley::GenericFamily::SystemUi,
    }
}

#[allow(dead_code)]
pub(crate) fn query_font_family(input: &stylo::SingleFontFamily) -> parley::QueryFamily<'_> {
    match input {
        stylo::SingleFontFamily::FamilyName(name) => {
            'ret: {
                let name = name.name.as_ref();

                // Legacy web compatibility
                #[cfg(target_vendor = "apple")]
                if name == "-apple-system" {
                    break 'ret parley::QueryFamily::Generic(parley::GenericFamily::SystemUi);
                }
                #[cfg(target_os = "macos")]
                if name == "BlinkMacSystemFont" {
                    break 'ret parley::QueryFamily::Generic(parley::GenericFamily::SystemUi);
                }

                break 'ret parley::QueryFamily::Named(name);
            }
        }
        stylo::SingleFontFamily::Generic(generic) => {
            parley::QueryFamily::Generic(self::generic_font_family(*generic))
        }
    }
}

pub(crate) fn font_weight(input: stylo::FontWeight) -> parley::FontWeight {
    parley::FontWeight::new(input.value())
}

pub(crate) fn font_width(input: stylo::FontStretch) -> parley::FontWidth {
    parley::FontWidth::from_percentage(input.0.to_float())
}

pub(crate) fn font_style(input: stylo::FontStyle) -> parley::FontStyle {
    match input {
        stylo::FontStyle::NORMAL => parley::FontStyle::Normal,
        stylo::FontStyle::ITALIC => parley::FontStyle::Italic,
        val => parley::FontStyle::Oblique(Some(val.oblique_degrees())),
    }
}

pub(crate) fn font_variations(input: &stylo::FontVariationSettings) -> Vec<parley::FontVariation> {
    input
        .0
        .iter()
        .map(|v| parley::FontVariation {
            tag: v.tag.0,
            value: v.value,
        })
        .collect()
}

pub(crate) fn white_space_collapse(input: stylo::WhiteSpaceCollapse) -> parley::WhiteSpaceCollapse {
    match input {
        stylo::WhiteSpaceCollapse::Collapse => parley::WhiteSpaceCollapse::Collapse,
        stylo::WhiteSpaceCollapse::Preserve => parley::WhiteSpaceCollapse::Preserve,

        // TODO: Implement PreserveBreaks and BreakSpaces modes
        stylo::WhiteSpaceCollapse::PreserveBreaks => parley::WhiteSpaceCollapse::Preserve,
        stylo::WhiteSpaceCollapse::BreakSpaces => parley::WhiteSpaceCollapse::Preserve,
    }
}

pub(crate) fn style(
    span_id: usize,
    style: &stylo::ComputedValues,
) -> parley::TextStyle<'static, TextBrush> {
    let font_styles = style.get_font();
    let text_styles = style.get_text();
    let itext_styles = style.get_inherited_text();

    // Convert font size and line height
    let font_size = font_styles.font_size.used_size.0.px();
    let line_height = match font_styles.line_height {
        stylo::LineHeight::Normal => parley::LineHeight::FontSizeRelative(1.2),
        stylo::LineHeight::Number(num) => parley::LineHeight::FontSizeRelative(num.0),
        stylo::LineHeight::Length(value) => parley::LineHeight::Absolute(value.0.px()),
    };

    let letter_spacing = itext_styles
        .letter_spacing
        .0
        .resolve(Length::new(font_size))
        .px();

    // Convert Bold/Italic
    let font_weight = self::font_weight(font_styles.font_weight);
    let font_style = self::font_style(font_styles.font_style);
    let font_width = self::font_width(font_styles.font_stretch);
    let font_variations = self::font_variations(&font_styles.font_variation_settings);

    // Convert font family
    let families: Vec<_> = font_styles
        .font_family
        .families
        .list
        .iter()
        .map(|family| match family {
            stylo::SingleFontFamily::FamilyName(name) => {
                'ret: {
                    let name = name.name.as_ref();

                    // Legacy web compatibility
                    #[cfg(target_vendor = "apple")]
                    if name == "-apple-system" {
                        break 'ret parley::FontFamily::Generic(parley::GenericFamily::SystemUi);
                    }
                    #[cfg(target_os = "macos")]
                    if name == "BlinkMacSystemFont" {
                        break 'ret parley::FontFamily::Generic(parley::GenericFamily::SystemUi);
                    }

                    break 'ret parley::FontFamily::Named(Cow::Owned(name.to_string()));
                }
            }
            stylo::SingleFontFamily::Generic(generic) => {
                parley::FontFamily::Generic(self::generic_font_family(*generic))
            }
        })
        .collect();

    // Convert text colour
    let color = itext_styles.color.as_color_color();

    // Text decorations
    let text_decoration_line = text_styles.text_decoration_line;
    let decoration_brush = style
        .get_text()
        .text_decoration_color
        .as_absolute()
        .map(ToColorColor::as_color_color)
        .map(TextBrush::from_color);

    // Wrapping and breaking
    let word_break = match itext_styles.word_break {
        stylo::WordBreak::Normal => parley::WordBreakStrength::Normal,
        stylo::WordBreak::BreakAll => parley::WordBreakStrength::BreakAll,
        stylo::WordBreak::KeepAll => parley::WordBreakStrength::KeepAll,
    };
    let overflow_wrap = match itext_styles.overflow_wrap {
        stylo::OverflowWrap::Normal => parley::OverflowWrap::Normal,
        stylo::OverflowWrap::BreakWord => parley::OverflowWrap::BreakWord,
        stylo::OverflowWrap::Anywhere => parley::OverflowWrap::Anywhere,
    };

    let css_weight = font_styles.font_weight.value();
    // Capture first family name (named or generic keyword) for backend selection
    let primary_family: std::sync::Arc<str> = families
        .get(0)
        .map(|f| match f {
            parley::FontFamily::Named(n) => n.as_ref().into(),
            parley::FontFamily::Generic(g) => match g {
                parley::GenericFamily::Monospace => "monospace".into(),
                parley::GenericFamily::Serif => "serif".into(),
                parley::GenericFamily::SansSerif => "sans-serif".into(),
                parley::GenericFamily::Cursive => "cursive".into(),
                parley::GenericFamily::Fantasy => "fantasy".into(),
                parley::GenericFamily::SystemUi => "system-ui".into(),
                parley::GenericFamily::UiSerif => "serif".into(),
                parley::GenericFamily::UiSansSerif => "sans-serif".into(),
                parley::GenericFamily::UiMonospace => "monospace".into(),
                parley::GenericFamily::UiRounded => "sans-serif".into(),
                parley::GenericFamily::Emoji => "emoji".into(),
                parley::GenericFamily::Math => "Cambria Math".into(),
                parley::GenericFamily::FangSong => "FangSong".into(),
            },
        })
        .unwrap_or_else(|| "".into());
    // Extract background color for inline elements (resolve GenericColor -> AbsoluteColor -> SRGB)
    let current_color = style.clone_color();
    let bg_color = 
        style.get_background().background_color.resolve_to_absolute(&current_color).as_color_color();
    let bg_brush = (bg_color.components[3] > 0.0).then(|| peniko::Brush::Solid(bg_color));

    // Inline background padding & radius: We don't yet have per-span CSS box model in parley.
    // As an interim step, derive padding from background presence and some element heuristics.
    // TODO: Once inline box metrics are exposed, map real CSS padding/border-radius here.
    let mut inline_padding = [0.0f32;4];
    let mut inline_radius = 0.0f32;
    if bg_brush.is_some() {
        // Small default vertical/horizontal padding similar to UA styles for <code>/<kbd>/<mark>
        inline_padding = [2.0,4.0,2.0,4.0];
        // Attempt a best-effort border-radius extraction using the top-left corner.
        // We don't have inline box dimensions yet; use font_size as a proxy for both axes so
        // pure lengths resolve correctly and percentages behave reasonably (percentage of ~em).
        let borders = style.get_border();
        let tl = &borders.border_top_left_radius; // type BorderCornerRadius
        // Each corner radius holds a pair (width,height) of LengthPercentage. Resolve both then take min.
        let resolve_em = CSSPixelLength::new(font_size as f32);
        let rx = tl.0.width.0.resolve(resolve_em).px() as f32;
        let ry = tl.0.height.0.resolve(resolve_em).px() as f32;
        inline_radius = rx.min(ry).clamp(0.0, font_size * 2.0);
    }
    parley::TextStyle {
        font_stack: parley::FontStack::List(Cow::Owned(families)),
        font_size,
        font_width,
        font_style,
        font_weight,
        font_variations: parley::FontSettings::List(Cow::Owned(font_variations)),
        font_features: parley::FontSettings::List(Cow::Borrowed(&[])),
        locale: Default::default(),
        brush: TextBrush::from_id_color_weight_family(span_id, color, css_weight as u16, primary_family)
            .with_background(bg_brush)
            .with_padding(inline_padding)
            .with_border_radius(inline_radius),
        has_underline: text_decoration_line.contains(TextDecorationLine::UNDERLINE),
        underline_offset: Default::default(),
        underline_size: Default::default(),
        underline_brush: decoration_brush.clone(),
        has_strikethrough: text_decoration_line.contains(TextDecorationLine::LINE_THROUGH),
        strikethrough_offset: Default::default(),
        strikethrough_size: Default::default(),
        strikethrough_brush: decoration_brush,
        line_height,
        word_spacing: Default::default(),
        letter_spacing,
        overflow_wrap,
        word_break,
    }
}
