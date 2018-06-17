use hex;

use {IntegerWidth, Result, Simple, Value, ByteString, TextString};

fn integer_to_diag(value: u64, bitwidth: IntegerWidth, s: &mut String) -> Result<()> {
    if bitwidth == IntegerWidth::Unknown || bitwidth == IntegerWidth::Zero {
        s.push_str(&value.to_string());
    } else {
        let encoding = match bitwidth {
            IntegerWidth::Eight => 0,
            IntegerWidth::Sixteen => 1,
            IntegerWidth::ThirtyTwo => 2,
            IntegerWidth::SixtyFour => 3,
            _ => unreachable!(),
        };
        s.push_str(&format!("{}_{}", value, encoding));
    }
    Ok(())
}

fn negative_to_diag(value: u64, bitwidth: IntegerWidth, s: &mut String) -> Result<()> {
    let value = -1i128 - i128::from(value);
    if bitwidth == IntegerWidth::Unknown || bitwidth == IntegerWidth::Zero {
        s.push_str(&value.to_string());
    } else {
        let encoding = match bitwidth {
            IntegerWidth::Eight => 0,
            IntegerWidth::Sixteen => 1,
            IntegerWidth::ThirtyTwo => 2,
            IntegerWidth::SixtyFour => 3,
            _ => unreachable!(),
        };
        s.push_str(&format!("{}_{}", value, encoding));
    }
    Ok(())
}

fn bytestring_to_diag(bytestring: &ByteString, s: &mut String) -> Result<()> {
    s.push_str(&format!("h'{}'", hex::encode(&bytestring.data)));
    Ok(())
}

fn textstring_to_diag(textstring: &TextString, s: &mut String) -> Result<()> {
    s.push('"');
    for c in textstring.data.chars() {
        if c == '\"' || c == '\\' {
            for c in c.escape_default() {
                s.push(c);
            }
        } else {
            s.push(c);
        }
    }
    s.push('"');

    Ok(())
}

fn indefinite_textstring_to_diag(textstrings: &[TextString], s: &mut String) -> Result<()> {
    s.push_str("(_");
    if textstrings.is_empty() {
        s.push(' ');
    }
    for textstring in textstrings {
        s.push(' ');
        textstring_to_diag(textstring, s)?;
    }
    s.push(')');

    Ok(())
}

fn simple_to_diag(simple: Simple, s: &mut String) -> Result<()> {
    match simple {
        Simple::FALSE => s.push_str("false"),
        Simple::TRUE => s.push_str("true"),
        Simple::NULL => s.push_str("null"),
        Simple::UNDEFINED => s.push_str("undefined"),
        Simple(value) => s.push_str(&format!("simple({})", value)),
    }
    Ok(())
}

fn value_to_diag(value: &Value, s: &mut String) -> Result<()> {
    match *value {
        Value::Integer { value, bitwidth } => integer_to_diag(value, bitwidth, s)?,
        Value::Negative { value, bitwidth } => negative_to_diag(value, bitwidth, s)?,
        Value::ByteString(ref bytestring) => bytestring_to_diag(bytestring, s)?,
        Value::TextString(ref textstring) => textstring_to_diag(textstring, s)?,
        Value::IndefiniteTextString(ref textstrings) => indefinite_textstring_to_diag(textstrings, s)?,
        Value::Simple(simple) => simple_to_diag(simple, s)?,
        _ => unimplemented!(),
    }
    Ok(())
}

impl Value {
    pub fn to_diag(&self) -> Result<String> {
        let mut s = String::with_capacity(128);
        value_to_diag(self, &mut s)?;
        Ok(s)
    }
}
