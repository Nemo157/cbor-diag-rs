use half::f16;

use std::fmt::{Display, Error, Formatter};

use super::Encoding;
use crate::{ByteString, DataItem, FloatWidth, IntegerWidth, Simple, Tag, TextString};

const I: &str = "";

#[derive(Copy, Clone, PartialEq, Eq)]
pub(crate) enum Layout {
    Pretty,
    Compact,
}

#[derive(Copy, Clone)]
pub(crate) struct Context<T> {
    value: T,
    layout: Layout,
    encoding: Encoding,
    indent: usize,
}

trait LengthEstimate {
    /// Can shortcircuit and return `max` if it is more than that
    fn estimate(&self, max: usize) -> usize;

    fn is_trivial(&self) -> bool {
        const MAX: usize = 60;
        self.estimate(MAX) < MAX
    }
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
            DataItem::Array { data, .. } => data.estimate(max.saturating_sub(4)) + 4,
            DataItem::Map { data, .. } => data.estimate(max.saturating_sub(4)) + 4,
            DataItem::IndefiniteByteString(strings) => strings.estimate(max.saturating_sub(4)) + 4,
            DataItem::IndefiniteTextString(strings) => strings.estimate(max.saturating_sub(4)) + 4,
            DataItem::Tag { tag, value, .. } => (tag, value).estimate(max),
        }
    }
}

impl<T: LengthEstimate + ?Sized> LengthEstimate for &T {
    fn estimate(&self, max: usize) -> usize {
        (**self).estimate(max)
    }
}

impl<T: LengthEstimate + ?Sized> LengthEstimate for Box<T> {
    fn estimate(&self, max: usize) -> usize {
        (**self).estimate(max)
    }
}

impl<T: LengthEstimate, U: LengthEstimate> LengthEstimate for (T, U) {
    fn estimate(&self, max: usize) -> usize {
        let mut len = self.0.estimate(max.saturating_sub(2)) + 2;
        if len < max {
            len += self.1.estimate(max.saturating_sub(len));
        }
        len
    }
}

impl<T: LengthEstimate> LengthEstimate for [T] {
    fn estimate(&self, max: usize) -> usize {
        let mut len = 0;
        for item in self {
            len += item.estimate(max.saturating_sub(len)) + 2;
            if len >= max {
                return len;
            }
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

impl<T> Context<T> {
    pub(crate) fn new(value: T, layout: Layout) -> Self {
        Self {
            value,
            layout,
            encoding: Encoding::Base16,
            indent: 0,
        }
    }

    fn with_encoding(self, encoding: Encoding) -> Self {
        Self { encoding, ..self }
    }

    fn is_pretty(&self) -> bool {
        self.layout == Layout::Pretty
    }

    fn wrap<U>(&self, value: U) -> Context<U> {
        Context {
            value,
            layout: self.layout,
            encoding: self.encoding,
            indent: self.indent,
        }
    }

    fn list<'a, U: LengthEstimate>(
        &self,
        begin: &'static str,
        items: &'a [U],
        end: &'static str,
        is_definite: bool,
    ) -> Context<List<'a, U>> {
        let is_trivial = items.is_trivial();
        self.wrap(List {
            begin,
            items,
            end,
            is_definite,
            is_trivial,
        })
    }

    fn indent(self) -> Self {
        Self {
            indent: self.indent + 4,
            ..self
        }
    }
}

impl<T> core::ops::Deref for Context<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl Display for Context<(bool, u64, IntegerWidth)> {
    #[fehler::throws]
    fn fmt(&self, f: &mut Formatter<'_>) {
        let (positive, value, bitwidth) = **self;
        let value = if positive {
            i128::from(value)
        } else {
            -1 - i128::from(value)
        };
        if bitwidth == IntegerWidth::Unknown || bitwidth == IntegerWidth::Zero {
            write!(f, "{}", value)?;
        } else {
            let encoding = match bitwidth {
                IntegerWidth::Eight => 0,
                IntegerWidth::Sixteen => 1,
                IntegerWidth::ThirtyTwo => 2,
                IntegerWidth::SixtyFour => 3,
                _ => unreachable!(),
            };
            write!(f, "{}_{}", value, encoding)?;
        }
    }
}

impl Display for Context<(f64, FloatWidth)> {
    #[fehler::throws]
    fn fmt(&self, f: &mut Formatter<'_>) {
        let (value, bitwidth) = **self;
        if value.is_nan() {
            f.write_str("NaN")?;
        } else if value.is_infinite() {
            if value.is_sign_negative() {
                f.write_str("-")?;
            }
            f.write_str("Infinity")?;
        } else {
            let value = match bitwidth {
                FloatWidth::Unknown | FloatWidth::SixtyFour => value.to_string(),
                FloatWidth::Sixteen => f16::from_f64(value).to_string(),
                FloatWidth::ThirtyTwo => (value as f32).to_string(),
            };
            f.write_str(&value)?;
            if !value.contains('.') && !value.contains('e') {
                f.write_str(".0")?;
            }
        }
        f.write_str(match bitwidth {
            FloatWidth::Unknown => "",
            FloatWidth::Sixteen => "_1",
            FloatWidth::ThirtyTwo => "_2",
            FloatWidth::SixtyFour => "_3",
        })?;
    }
}

impl Display for Context<Simple> {
    #[fehler::throws]
    fn fmt(&self, f: &mut Formatter<'_>) {
        match **self {
            Simple::FALSE => f.write_str("false")?,
            Simple::TRUE => f.write_str("true")?,
            Simple::NULL => f.write_str("null")?,
            Simple::UNDEFINED => f.write_str("undefined")?,
            Simple(value) => write!(f, "simple({})", value)?,
        }
    }
}

impl Display for Context<&ByteString> {
    #[fehler::throws]
    fn fmt(&self, f: &mut Formatter<'_>) {
        match self.encoding {
            Encoding::Base64Url => {
                write!(
                    f,
                    "b64'{}'",
                    data_encoding::BASE64URL_NOPAD.encode(&self.data)
                )?;
            }
            Encoding::Base64 => {
                write!(f, "b64'{}'", data_encoding::BASE64.encode(&self.data))?;
            }
            Encoding::Base16 => {
                write!(f, "h'{}'", data_encoding::HEXLOWER.encode(&self.data))?;
            }
        }
    }
}

impl Display for Context<&TextString> {
    #[fehler::throws]
    fn fmt(&self, f: &mut Formatter<'_>) {
        f.write_str("\"")?;
        for c in self.data.chars() {
            if c == '\"' || c == '\\' {
                write!(f, "{}", c.escape_default())?;
            } else {
                write!(f, "{c}")?;
            }
        }
        f.write_str("\"")?;
    }
}

struct List<'a, T> {
    begin: &'static str,
    items: &'a [T],
    end: &'static str,
    is_definite: bool,
    is_trivial: bool,
}

impl<T> Display for Context<List<'_, T>>
where
    for<'a> Context<&'a T>: Display,
{
    #[fehler::throws]
    fn fmt(&self, f: &mut Formatter<'_>) {
        let Context { mut indent, .. } = *self;
        f.write_str(self.begin)?;
        if !self.is_definite {
            f.write_str("_")?;
            if self.is_pretty() && self.is_trivial {
                f.write_str(" ")?;
            }
        }
        indent += 4;
        let mut items = self.items.iter();
        if let Some(item) = items.next() {
            if self.is_pretty() && !self.is_trivial {
                write!(f, "\n{I:indent$}")?;
            }
            self.wrap(item).indent().fmt(f)?;
        }
        for item in items {
            f.write_str(",")?;
            if self.is_pretty() {
                if self.is_trivial {
                    f.write_str(" ")?;
                } else {
                    write!(f, "\n{I:indent$}")?;
                }
            }
            self.wrap(item).fmt(f)?;
        }
        indent -= 4;
        if self.is_pretty() && !self.is_trivial {
            write!(f, ",\n{I:indent$}")?;
        }
        f.write_str(self.end)?;
    }
}

impl Display for Context<&(DataItem, DataItem)> {
    #[fehler::throws]
    fn fmt(&self, f: &mut Formatter<'_>) {
        let (key, value) = (self.wrap(&self.0), self.wrap(&self.1));
        if self.is_pretty() {
            write!(f, "{key}: {value}")?;
        } else {
            write!(f, "{key}:{value}")?;
        }
    }
}

impl Display for Context<(Tag, IntegerWidth, &DataItem)> {
    #[fehler::throws]
    fn fmt(&self, f: &mut Formatter<'_>) {
        let (tag, width, value) = **self;

        write!(f, "{}(", self.wrap((true, tag.0, width)))?;

        match tag {
            Tag::ENCODED_BASE64URL => {
                self.wrap(value).with_encoding(Encoding::Base64Url).fmt(f)?;
            }
            Tag::ENCODED_BASE64 => {
                self.wrap(value).with_encoding(Encoding::Base64).fmt(f)?;
            }
            Tag::ENCODED_BASE16 => {
                self.wrap(value).with_encoding(Encoding::Base16).fmt(f)?;
            }
            Tag::ENCODED_CBOR => {
                if let DataItem::ByteString(ByteString { data, .. }) = value {
                    if data.is_empty() {
                        self.list::<DataItem>("<<", &[], ">>", true).fmt(f)?;
                    } else if let Ok(item) = crate::parse_bytes(data) {
                        let items = &[item];
                        self.list("<<", items, ">>", true).fmt(f)?;
                    } else {
                        self.wrap(value).fmt(f)?;
                    }
                } else {
                    self.wrap(value).fmt(f)?;
                }
            }
            Tag::ENCODED_CBOR_SEQ => {
                if let DataItem::ByteString(ByteString { ref data, bitwidth }) = *value {
                    let mut data = data.as_slice();
                    let mut items = Vec::new();
                    while let Ok(Some((item, len))) = crate::parse_bytes_partial(data) {
                        let (_, rest) = data.split_at(len);
                        data = rest;
                        items.push(item);
                    }
                    self.list("<<", &items, ">>", true).fmt(f)?;
                    if !data.is_empty() {
                        if self.is_pretty() {
                            f.write_str(" ")?;
                        }
                        self.wrap(&DataItem::ByteString(ByteString {
                            data: data.into(),
                            bitwidth,
                        }))
                        .fmt(f)?;
                    }
                } else {
                    self.wrap(value).fmt(f)?;
                }
            }
            _ => {
                self.wrap(value).fmt(f)?;
            }
        }

        f.write_str(")")?;
    }
}

impl Display for Context<&DataItem> {
    #[fehler::throws]
    fn fmt(&self, f: &mut Formatter<'_>) {
        match ***self {
            DataItem::Integer { value, bitwidth } => {
                self.wrap((true, value, bitwidth)).fmt(f)?;
            }
            DataItem::Negative { value, bitwidth } => {
                self.wrap((false, value, bitwidth)).fmt(f)?;
            }
            DataItem::ByteString(ref bytestring) => {
                self.wrap(bytestring).fmt(f)?;
            }
            DataItem::IndefiniteByteString(ref strings) => {
                self.list("(", strings, ")", false).fmt(f)?;
            }
            DataItem::TextString(ref string) => {
                self.wrap(string).fmt(f)?;
            }
            DataItem::IndefiniteTextString(ref strings) => {
                self.list("(", strings, ")", false).fmt(f)?;
            }
            DataItem::Array { ref data, bitwidth } => {
                self.list("[", data, "]", bitwidth.is_some()).fmt(f)?;
            }
            DataItem::Map { ref data, bitwidth } => {
                self.list("{", data, "}", bitwidth.is_some()).fmt(f)?;
            }
            DataItem::Tag {
                tag,
                bitwidth,
                ref value,
            } => {
                self.wrap((tag, bitwidth, &**value)).fmt(f)?;
            }
            DataItem::Float { value, bitwidth } => {
                self.wrap((value, bitwidth)).fmt(f)?;
            }
            DataItem::Simple(simple) => {
                self.wrap(simple).fmt(f)?;
            }
        }
    }
}

impl DataItem {
    pub fn diagnostic(&self) -> impl Display + '_ {
        Context::new(self, Layout::Compact)
    }

    pub fn diagnostic_pretty(&self) -> impl Display + '_ {
        Context::new(self, Layout::Pretty)
    }
}
