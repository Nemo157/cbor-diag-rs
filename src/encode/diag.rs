use base64::{self, display::Base64Display};
use half::f16;
use hex;
use ::stylish::{Color, Intensity};

use super::Encoding;
use {ByteString, DataItem, FloatWidth, IntegerWidth, Simple, Tag, TextString};

#[derive(Copy, Clone, PartialEq, Eq)]
pub(crate) enum Layout {
    Pretty,
    Compact,
}

pub(crate) struct Contextual<T> {
    layout: Layout,
    encoding: Encoding,
    indent: usize,
    inner: T,
}

trait LengthEstimate {
    /// Can shortcircuit and return `max` if it is more than that
    fn estimate(&self, max: usize) -> usize;
}

fn is_trivial(value: &impl LengthEstimate) -> bool {
    const MAX: usize = 60;
    value.estimate(MAX) < MAX
}

impl LengthEstimate for DataItem {
    fn estimate(&self, max: usize) -> usize {
        match self {
            DataItem::Integer { value, .. } => value.to_string().len() + 2,
            DataItem::Negative { value, .. } => value.to_string().len() + 3,
            DataItem::Float { value, .. } => value.to_string().len() + 3,
            DataItem::Simple(value) => value.estimate(max),
            DataItem::ByteString(value) => value.estimate(max),
            DataItem::TextString(value) => value.estimate(max),
            DataItem::Array { data, .. } => {
                let mut len = 4;
                for item in data {
                    len += item.estimate(max - len) + 2;
                    if len >= max {
                        return len;
                    }
                }
                len
            }
            DataItem::Map { data, .. } => {
                let mut len = 4;
                for entry in data {
                    len += entry.estimate(max - len) + 2;
                    if len >= max {
                        return len;
                    }
                }
                len
            }
            DataItem::IndefiniteByteString(strings) => {
                let mut len = 4;
                for string in strings {
                    len += string.estimate(max - len) + 2;
                    if len >= max {
                        return len;
                    }
                }
                len
            }
            DataItem::IndefiniteTextString(strings) => {
                let mut len = 4;
                for string in strings {
                    len += string.estimate(max - len) + 2;
                    if len >= max {
                        return len;
                    }
                }
                len
            }
            DataItem::Tag { tag, value, .. } => (tag, value).estimate(max),
        }
    }
}

impl<T: LengthEstimate + ?Sized> LengthEstimate for &T {
    fn estimate(&self, max: usize) -> usize {
        (&**self).estimate(max)
    }
}

impl<T: LengthEstimate + ?Sized> LengthEstimate for Box<T> {
    fn estimate(&self, max: usize) -> usize {
        (&**self).estimate(max)
    }
}

impl<T: LengthEstimate, U: LengthEstimate> LengthEstimate for (T, U) {
    fn estimate(&self, max: usize) -> usize {
        let mut len = self.0.estimate(max);
        if len < max {
            len += self.1.estimate(max - len);
        }
        len
    }
}

impl LengthEstimate for ByteString {
    fn estimate(&self, _: usize) -> usize {
        self.data.len() * 2 + 4
    }
}

impl LengthEstimate for TextString {
    fn estimate(&self, _: usize) -> usize {
        self.data.len() + 2
    }
}

impl LengthEstimate for Tag {
    fn estimate(&self, _: usize) -> usize {
        self.0.to_string().len() + 2
    }
}

impl LengthEstimate for Simple {
    fn estimate(&self, _: usize) -> usize {
        self.0.to_string().len() + 8
    }
}

impl<T> Contextual<T> {
    pub(crate) fn new(layout: Layout, inner: T) -> Self {
        Self {
            layout,
            inner,
            encoding: Encoding::Base16,
            indent: 0,
        }
    }

    pub(crate) fn with_encoding(&self, encoding: Encoding) -> Self where T: Copy {
        Self {
            layout: self.layout,
            encoding,
            indent: self.indent,
            inner: self.inner,
        }
    }

    pub(crate) fn wrap<U>(&self, inner: U) -> Contextual<U> {
        Contextual {
            layout: self.layout,
            encoding: self.encoding,
            indent: self.indent,
            inner,
        }
    }

    fn pretty(&self) -> bool {
        self.layout == Layout::Pretty
    }

    fn with_indent(&self, indent: usize) -> Contextual<&T> {
        Contextual {
            layout: self.layout,
            encoding: self.encoding,
            indent: self.indent + indent,
            inner: &self.inner,
        }
    }

    fn indent(&self) -> String {
        let mut output = String::new();
        for _ in 0..self.indent {
            output.push(' ');
        }
        output
    }
}

impl<T> core::ops::Deref for Contextual<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

struct Integer {
    value: u64,
    bitwidth: IntegerWidth,
}

struct Negative {
    value: u64,
    bitwidth: IntegerWidth,
}

struct Float {
    value: f64,
    bitwidth: FloatWidth,
}

struct Container<'a, T> {
    begin: &'a str,
    items: &'a [T],
    end: &'a str,
    definite: bool,
    trivial: bool,
}

struct Tagged<'a> {
    tag: Tag,
    bitwidth: IntegerWidth,
    value: &'a DataItem,
}

impl stylish::Display for Contextual<&Integer> {
    fn fmt(&self, f: &mut stylish::Formatter<'_>) -> std::fmt::Result {
        if let IntegerWidth::Unknown | IntegerWidth::Zero = self.bitwidth {
            self.value.fmt(f)?;
        } else {
            let encoding = match self.bitwidth {
                IntegerWidth::Eight => 0,
                IntegerWidth::Sixteen => 1,
                IntegerWidth::ThirtyTwo => 2,
                IntegerWidth::SixtyFour => 3,
                _ => return Ok(()),
            };
            f.write_fmt(&stylish::Arguments {
                pieces: &[
                    stylish::Argument::Val(&self.value),
                    stylish::Argument::With {
                        restyle: &Intensity::Faint,
                        arguments: stylish::Arguments {
                            pieces: &[stylish::Argument::Lit("_"), stylish::Argument::Val(&encoding)],
                        }
                    }
                ]
            })?;
        }
        Ok(())
    }
}

impl stylish::Display for Contextual<&Negative> {
    fn fmt(&self, f: &mut stylish::Formatter<'_>) -> std::fmt::Result {
        let value = -1i128 - i128::from(self.value);
        if let IntegerWidth::Unknown | IntegerWidth::Zero = self.bitwidth {
            value.fmt(f)?;
        } else {
            let encoding = match self.bitwidth {
                IntegerWidth::Eight => 0,
                IntegerWidth::Sixteen => 1,
                IntegerWidth::ThirtyTwo => 2,
                IntegerWidth::SixtyFour => 3,
                _ => return Ok(()),
            };
            f.write_fmt(&stylish::Arguments {
                pieces: &[
                    stylish::Argument::Val(&value),
                    stylish::Argument::With {
                        restyle: &Intensity::Faint,
                        arguments: stylish::Arguments {
                            pieces: &[stylish::Argument::Lit("_"), stylish::Argument::Val(&encoding)],
                        }
                    }
                ]
            })?;
        }
        Ok(())
    }
}

impl stylish::Display for Contextual<&Float> {
    fn fmt(&self, f: &mut stylish::Formatter<'_>) -> std::fmt::Result {
        if self.value.is_nan() {
            f.write_str("NaN")?;
        } else if self.value.is_infinite() {
            if self.value.is_sign_negative() {
                f.write_str("-")?;
            }
            f.write_str("Infinity")?;
        } else {
            let value = match self.bitwidth {
                FloatWidth::Unknown | FloatWidth::SixtyFour => self.value.to_string(),
                FloatWidth::Sixteen => f16::from_f64(self.value).to_string(),
                FloatWidth::ThirtyTwo => (self.value as f32).to_string(),
            };
            f.write_str(&value)?;
            if !value.contains('.') && !value.contains('e') {
                f.write_str(".0")?;
            }
        }
        f.with(Intensity::Faint).write_str(match self.bitwidth {
            FloatWidth::Unknown => "",
            FloatWidth::Sixteen => "_1",
            FloatWidth::ThirtyTwo => "_2",
            FloatWidth::SixtyFour => "_3",
        })?;
        Ok(())
    }
}

impl stylish::Display for Contextual<&Simple> {
    fn fmt(&self, f: &mut stylish::Formatter<'_>) -> std::fmt::Result {
        match ***self {
            Simple::FALSE => f.with(Color::Green).write_str("false")?,
            Simple::TRUE => f.with(Color::Green).write_str("true")?,
            Simple::NULL => f.with(Color::Red).write_str("null")?,
            Simple::UNDEFINED => {
                f.with(Color::Red).write_str("undefined")?;
            }
            Simple(value) => f.with(Color::Magenta).write_fmt(&stylish::Arguments {
                pieces: &[
                    stylish::Argument::Lit("simple("),
                    stylish::Argument::Val(&value),
                    stylish::Argument::Lit(")"),
                ]
            })?,
        }
        Ok(())
    }
}

impl stylish::Display for Contextual<&TextString> {
    fn fmt(&self, f: &mut stylish::Formatter<'_>) -> std::fmt::Result {
        let mut f = f.with(Color::Blue);

        f.write_str("\"")?;
        for c in self.data.chars() {
            if c == '\"' || c == '\\' {
                for c in c.escape_default() {
                    f.write_char(c)?;
                }
            } else {
                f.write_char(c)?;
            }
        }
        f.write_str("\"")?;

        Ok(())
    }
}

impl stylish::Display for Contextual<&ByteString> {
    fn fmt(&self, f: &mut stylish::Formatter<'_>) -> std::fmt::Result {
        let mut f = f.with(Color::Yellow);

        f.with(Intensity::Faint).write_str(match self.encoding {
            Encoding::Base64Url | Encoding::Base64 => "b64'",
            Encoding::Base16 => "h'",
        })?;

        match self.encoding {
            Encoding::Base64Url => {
                Base64Display::with_config(&self.data, base64::URL_SAFE_NO_PAD).fmt(&mut f)?;
            }
            Encoding::Base64 => {
                Base64Display::with_config(&self.data, base64::STANDARD_NO_PAD).fmt(&mut f)?;
            }
            Encoding::Base16 => {
                f.write_str(&hex::encode(&self.data))?;
            }
        }
        f.with(Intensity::Faint).write_str("'")?;

        Ok(())
    }
}

impl stylish::Display for Contextual<&Tagged<'_>> {
    fn fmt(&self, f: &mut stylish::Formatter<'_>) -> std::fmt::Result {
        let mut g = f.with(Color::Cyan);
        self.tag.0.fmt(&mut g)?;
        let encoding = match self.bitwidth {
            IntegerWidth::Eight => Some(0),
            IntegerWidth::Sixteen => Some(1),
            IntegerWidth::ThirtyTwo => Some(2),
            IntegerWidth::SixtyFour => Some(3),
            IntegerWidth::Unknown | IntegerWidth::Zero => None,
        };
        if let Some(encoding) = encoding {
            g.with(Intensity::Faint).write_fmt(&stylish::Arguments {
                pieces: &[
                    stylish::Argument::Lit("_"),
                    stylish::Argument::Val(&encoding),
                ]
            })?;
        }
        g.write_str("(")?;

        match self.tag {
            Tag::ENCODED_BASE64URL => {
                self.with_encoding(Encoding::Base64Url).wrap(self.value).fmt(f)?;
            }
            Tag::ENCODED_BASE64 => {
                self.with_encoding(Encoding::Base64).wrap(self.value).fmt(f)?;
            }
            Tag::ENCODED_BASE16 => {
                self.with_encoding(Encoding::Base16).wrap(self.value).fmt(f)?;
            }
            _ => {
                self.wrap(self.value).fmt(f)?;
            }
        }

        f.with(Color::Cyan).write_str(")")?;

        Ok(())
    }
}

impl<'a, T> stylish::Display for Contextual<&Container<'a, T>> where Contextual<&'a T>: stylish::Display {
    fn fmt(&self, f: &mut stylish::Formatter<'_>) -> std::fmt::Result {
        f.with(Intensity::Bold).write_str(self.begin)?;
        if !self.definite {
            f.with(Intensity::Normal).write_str("_")?;
            if self.trivial && self.pretty() {
                f.write_str(" ")?;
            }
        }
        let this = self.with_indent(if self.trivial { 0 } else { 4 });
        let mut items = this.items.iter();
        if let Some(item) = items.next() {
            if this.pretty() && !this.trivial {
                f.write_str("\n")?;
                f.write_str(&this.indent())?;
            }
            this.wrap(item).fmt(f)?;
        }
        for item in items {
            f.with(Intensity::Normal).write_str(",")?;
            if this.pretty() {
                if this.trivial {
                    f.write_str(" ")?;
                } else {
                    f.write_str("\n")?;
                    f.write_str(&this.indent())?;
                }
            }
            this.wrap(item).fmt(f)?;
        }
        if self.pretty() && !this.trivial {
            f.with(Intensity::Normal).write_str(",")?;
            f.write_str("\n")?;
            f.write_str(&self.indent())?;
        }
        f.with(Intensity::Bold).write_str(self.end)?;
        Ok(())
    }
}

// Map key-value pairs
impl stylish::Display for Contextual<&(DataItem, DataItem)> {
    fn fmt(&self, f: &mut stylish::Formatter<'_>) -> std::fmt::Result {
        self.wrap(&self.0).fmt(&mut f.with(Intensity::Bold))?;
        f.write_str(":")?;
        if self.pretty() {
            f.write_str(" ")?;
        }
        self.wrap(&self.1).fmt(f)?;
        Ok(())
    }
}

impl stylish::Display for Contextual<&DataItem> {
    fn fmt(&self, f: &mut stylish::Formatter<'_>) -> std::fmt::Result {
        match ***self {
            DataItem::Integer { value, bitwidth } => {
                self.wrap(&Integer { value, bitwidth }).fmt(f)?;
            }
            DataItem::Negative { value, bitwidth } => {
                self.wrap(&Negative { value, bitwidth }).fmt(f)?;
            }
            DataItem::Float { value, bitwidth } => {
                self.wrap(&Float { value, bitwidth }).fmt(f)?;
            }
            DataItem::Simple(ref value) => {
                self.wrap(value).fmt(f)?;
            }
            DataItem::Array {
                ref data,
                ref bitwidth,
            } => {
                self.wrap(&Container {
                    begin: "[",
                    items: data,
                    end: "]",
                    definite: bitwidth.is_some(),
                    trivial: is_trivial(**self),
                }).fmt(f)?;
            }
            DataItem::Map {
                ref data,
                ref bitwidth,
            } => {
                self.wrap(&Container {
                    begin: "{",
                    items: data,
                    end: "}",
                    definite: bitwidth.is_some(),
                    trivial: is_trivial(**self),
                }).fmt(f)?;
            }
            DataItem::TextString(ref textstring) => {
                self.wrap(textstring).fmt(f)?;
            }
            DataItem::IndefiniteTextString(ref textstrings) => {
                self.wrap(&Container {
                    begin: "(",
                    items: textstrings,
                    end: ")",
                    definite: false,
                    trivial: is_trivial(**self),
                }).fmt(f)?;
            }
            DataItem::ByteString(ref bytestring) => {
                self.wrap(bytestring).fmt(f)?;
            }
            DataItem::IndefiniteByteString(ref bytestrings) => {
                self.wrap(&Container {
                    begin: "(",
                    items: bytestrings,
                    end: ")",
                    definite: false,
                    trivial: is_trivial(**self),
                }).fmt(f)?;
            }
            DataItem::Tag {
                tag,
                bitwidth,
                ref value,
            } => {
                self.wrap(&Tagged { tag, bitwidth, value }).fmt(f)?;
            }
        }
        Ok(())
    }
}

impl DataItem {
    pub fn compact_diag(&self) -> impl stylish::Display + '_ {
        Contextual::new(Layout::Compact, self)
    }

    pub fn pretty_diag(&self) -> impl stylish::Display + '_ {
        Contextual::new(Layout::Pretty, self)
    }
}
