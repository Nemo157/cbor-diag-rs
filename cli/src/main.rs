use anyhow::anyhow;
use std::io::{self, Read, Write};
use strum::VariantNames;

#[derive(Copy, Clone, Debug, Eq, PartialEq, strum::EnumString, strum::EnumVariantNames)]
#[strum(serialize_all = "snake_case")]
enum From {
    Auto,
    Hex,
    Bytes,
    Diag,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, strum::EnumString, strum::EnumVariantNames)]
#[strum(serialize_all = "snake_case")]
enum To {
    Annotated,
    Hex,
    Bytes,
    Diag,
    Compact,
}

#[derive(Copy, Clone, Debug, strum::EnumString, strum::EnumVariantNames)]
#[strum(serialize_all = "snake_case")]
enum Color {
    Auto,
    Always,
    Never,
}

#[derive(Debug, structopt::StructOpt)]
#[structopt(name = "cbor-diag", setting = structopt::clap::AppSettings::ColoredHelp)]
/// A utility for converting between binary, diagnostic, hex and annotated hex
/// formats for CBOR.
struct Args {
    /// What format to attempt to parse the input as
    #[structopt(long, default_value = "auto", possible_values(From::VARIANTS))]
    from: From,

    /// What format to output
    #[structopt(long, default_value = "diag", possible_values(To::VARIANTS))]
    to: To,

    /// When to enable color
    #[structopt(long, default_value = "auto", possible_values(Color::VARIANTS))]
    color: Color,

    /// Parse a series of undelimited CBOR data items in binary format (a.k.a. the `cbor-seq` data
    /// type).
    #[structopt(long, conflicts_with("from"))]
    seq: bool,
}

trait ReadExt: Read {
    fn read_to_vec(&mut self, buffer: &mut Vec<u8>) -> io::Result<bool> {
        let offset = buffer.len();
        buffer.resize(offset + 10 * 1024, 0);
        let len = self.read(&mut buffer[offset..])?;
        buffer.resize(offset + len, 0);
        Ok(len != 0)
    }
}

impl<R: Read> ReadExt for R {}

fn output_item(value: cbor_diag::DataItem, to: To, color: Color, mut output: impl Write) -> anyhow::Result<()> {
    use stylish::Write;

    match (to, color) {
        (To::Annotated, _) => {
            output.write_all(value.to_hex().as_bytes())?;
        }
        (To::Hex, _) => {
            output.write_all(hex::encode(value.to_bytes()).as_bytes())?;
        }
        (To::Bytes, _) => {
            output.write_all(&value.to_bytes())?;
        }
        (To::Diag, Color::Auto) | (To::Diag, Color::Always) => {
            let mut output = stylish::ansi::Write::new(output);
            output.write_fmt(&stylish::Arguments {
                pieces: &[
                    stylish::Argument::Val(&value.pretty_diag()),
                    stylish::Argument::Lit("\n"),
                ]
            })?;
        }
        (To::Diag, Color::Never) => {
            let mut output = stylish::plain::Write::new(output);
            output.write_fmt(&stylish::Arguments {
                pieces: &[
                    stylish::Argument::Val(&value.pretty_diag()),
                    stylish::Argument::Lit("\n"),
                ]
            })?;
        }
        (To::Compact, Color::Auto) | (To::Compact, Color::Always) => {
            let mut output = stylish::ansi::Write::new(output);
            output.write_fmt(&stylish::Arguments {
                pieces: &[
                    stylish::Argument::Val(&value.compact_diag()),
                    stylish::Argument::Lit("\n"),
                ]
            })?;
        }
        (To::Compact, Color::Never) => {
            let mut output = stylish::plain::Write::new(output);
            output.write_fmt(&stylish::Arguments {
                pieces: &[
                    stylish::Argument::Val(&value.compact_diag()),
                    stylish::Argument::Lit("\n"),
                ]
            })?;
        }
    };

    Ok(())
}

#[paw::main]
fn main(args: Args) -> anyhow::Result<()> {
    let input = std::io::stdin();
    let mut input = input.lock();

    let output = std::io::stdout();
    let mut output = output.lock();

    if args.seq {
        let mut data = Default::default();

        while input.read_to_vec(&mut data)? {
            while let Some((value, len)) = cbor_diag::parse_bytes_partial(&data)? {
                output_item(value, args.to, args.color, &mut output)?;
                if args.to != To::Bytes && args.to != To::Compact {
                    output.write_all(b"\n")?;
                }
                data.drain(..len);
            }
        }

        if !data.is_empty() {
            return Err(anyhow!("{} bytes remaining after last item", data.len()));
        }
    } else {
        let data = {
            let mut data = Default::default();
            input.read_to_end(&mut data)?;
            data
        };

        let value = match args.from {
            From::Auto => cbor_diag::parse_bytes(&data)
                .ok()
                .or_else(|| {
                    String::from_utf8(data).ok().and_then(|data| {
                        cbor_diag::parse_hex(&data)
                            .ok()
                            .or_else(|| cbor_diag::parse_diag(&data).ok())
                    })
                })
                .ok_or_else(|| anyhow!("Failed all parsers"))?,
            From::Hex => {
                let data = String::from_utf8(data)?;
                cbor_diag::parse_hex(data)?
            }
            From::Bytes => cbor_diag::parse_bytes(data)?,
            From::Diag => {
                let data = String::from_utf8(data)?;
                cbor_diag::parse_diag(data)?
            }
        };

        output_item(value, args.to, args.color, &mut output)?;
    }

    Ok(())
}
