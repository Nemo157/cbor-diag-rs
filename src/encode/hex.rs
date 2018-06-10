use std::{ascii, cmp};

use hex;

use {IntegerWidth, Result, Simple, Value};

fn integer_to_hex(value: u64, mut bitwidth: IntegerWidth, s: &mut String) -> Result<()> {
    if bitwidth == IntegerWidth::Unknown {
        bitwidth = if value < 24 {
            IntegerWidth::Zero
        } else if value < u64::from(u8::max_value()) {
            IntegerWidth::Eight
        } else if value < u64::from(u16::max_value()) {
            IntegerWidth::Sixteen
        } else if value < u64::from(u32::max_value()) {
            IntegerWidth::ThirtyTwo
        } else {
            IntegerWidth::SixtyFour
        };
    }

    match bitwidth {
        IntegerWidth::Unknown => unreachable!(),
        IntegerWidth::Zero => s.push_str(&format!("{:02x}", value)),
        IntegerWidth::Eight => s.push_str(&format!("18 {:02x}", value)),
        IntegerWidth::Sixteen => s.push_str(&format!("19 {:04x}", value)),
        IntegerWidth::ThirtyTwo => s.push_str(&format!("1a {:08x}", value)),
        IntegerWidth::SixtyFour => s.push_str(&format!("1b {:016x}", value)),
    }

    s.push_str(&format!(" # unsigned({})", value));
    Ok(())
}

fn negative_to_hex(value: u64, mut bitwidth: IntegerWidth, s: &mut String) -> Result<()> {
    if bitwidth == IntegerWidth::Unknown {
        bitwidth = if value < 24 {
            IntegerWidth::Zero
        } else if value < u64::from(u8::max_value()) {
            IntegerWidth::Eight
        } else if value < u64::from(u16::max_value()) {
            IntegerWidth::Sixteen
        } else if value < u64::from(u32::max_value()) {
            IntegerWidth::ThirtyTwo
        } else {
            IntegerWidth::SixtyFour
        };
    }

    match bitwidth {
        IntegerWidth::Unknown => unreachable!(),
        IntegerWidth::Zero => s.push_str(&format!("{:02x}", value + 0x20)),
        IntegerWidth::Eight => s.push_str(&format!("38 {:02x}", value)),
        IntegerWidth::Sixteen => s.push_str(&format!("39 {:04x}", value)),
        IntegerWidth::ThirtyTwo => s.push_str(&format!("3a {:08x}", value)),
        IntegerWidth::SixtyFour => s.push_str(&format!("3b {:016x}", value)),
    }

    s.push_str(&format!(" # negative({})", value));
    Ok(())
}

fn bytestring_to_hex(data: &[u8], bitwidth: Option<IntegerWidth>, s: &mut String) -> Result<()> {
    let length = data.len() as u64;

    let mut bitwidth = bitwidth.expect("indefinite length is unimplemented");

    if bitwidth == IntegerWidth::Unknown {
        bitwidth = if length < 24 {
            IntegerWidth::Zero
        } else if length < u64::from(u8::max_value()) {
            IntegerWidth::Eight
        } else if length < u64::from(u16::max_value()) {
            IntegerWidth::Sixteen
        } else if length < u64::from(u32::max_value()) {
            IntegerWidth::ThirtyTwo
        } else {
            IntegerWidth::SixtyFour
        };
    }

    match bitwidth {
        IntegerWidth::Unknown => unreachable!(),
        IntegerWidth::Zero => s.push_str(&format!("{:02x} ", length + 0x40)),
        IntegerWidth::Eight => s.push_str(&format!("58 {:02x}", length)),
        IntegerWidth::Sixteen => s.push_str(&format!("59 {:04x}", length)),
        IntegerWidth::ThirtyTwo => s.push_str(&format!("5a {:08x}", length)),
        IntegerWidth::SixtyFour => s.push_str(&format!("5b {:016x}", length)),
    }

    let length_width = match bitwidth {
        IntegerWidth::Unknown => unreachable!(),
        IntegerWidth::Zero => 0,
        IntegerWidth::Eight => 2,
        IntegerWidth::Sixteen => 4,
        IntegerWidth::ThirtyTwo => 8,
        IntegerWidth::SixtyFour => 16,
    };

    let data_width = cmp::min(data.len() * 2, 32);
    let base_width = cmp::max(data_width, length_width);

    s.push_str(&format!(
        "{blank:width$} # bytes({length})\n",
        blank="",
        width=base_width.saturating_sub(length_width),
        length=length));

    for line in data.chunks(16) {
        let text: String = line
            .iter()
            .cloned()
            .flat_map(ascii::escape_default)
            .map(char::from)
            .collect();

        s.push_str(&format!(
            r#"   {data}{blank:width$} # "{text}"{n}"#,
            blank="",
            width=base_width.saturating_sub(line.len() * 2),
            data=hex::encode(line),
            text=text,
            n="\n"));
    }

    if data.is_empty() {
        s.push_str(&format!(
            r#"   {blank:width$} # ""{n}"#,
            blank="",
            width=base_width,
            n="\n"));
    }

    Ok(())
}

fn simple_to_hex(simple: Simple, s: &mut String) -> Result<()> {
    let Simple(value) = simple;

    if value < 24 {
        s.push_str(&format!("{:02x} # ", 0b1110_0000 | value));
    } else {
        s.push_str(&format!("f8 {:02x} # ", value));
    }

    match simple {
        Simple::FALSE => s.push_str("false, "),
        Simple::TRUE => s.push_str("true, "),
        Simple::NULL => s.push_str("null, "),
        Simple::UNDEFINED => s.push_str("undefined, "),
        Simple(24...32) => s.push_str("reserved, "),
        _ => s.push_str("unassigned, "),
    }

    s.push_str(&format!("simple({})", value));
    Ok(())
}

fn to_hex(value: &Value, s: &mut String) -> Result<()> {
    match *value {
        Value::Integer { value, bitwidth } => integer_to_hex(value, bitwidth, s)?,
        Value::Negative { value, bitwidth } => negative_to_hex(value, bitwidth, s)?,
        Value::ByteString { ref data, bitwidth } => bytestring_to_hex(data, bitwidth, s)?,
        Value::Simple(simple) => simple_to_hex(simple, s)?,
        _ => unimplemented!(),
    }
    Ok(())
}

impl Value {
    pub fn to_hex(&self) -> Result<String> {
        let mut s = String::with_capacity(128);
        to_hex(self, &mut s)?;
        Ok(s)
    }
}
