//! The bounded subset of VRF's textual KV3 used by animation resources.
use anyhow::{bail, ensure, Context, Result};
use serde_json::{Map, Value};

pub fn parse(text: &str) -> Result<Value> {
    parse_document(text, None, 64 * 1024 * 1024)
}

/// Keep large PHYS blobs as bytes instead of allocating one JSON value per byte.
pub struct BinaryDocument {
    pub value: Value,
    blobs: Vec<Vec<u8>>,
}
impl BinaryDocument {
    pub fn binary(&self, reference: &Value) -> Result<&[u8]> {
        let object = reference
            .as_object()
            .context("expected KV3 binary reference")?;
        ensure!(object.len() == 1, "invalid KV3 binary reference");
        let index = object
            .get("$binary")
            .and_then(Value::as_u64)
            .context("invalid KV3 binary reference")?;
        self.blobs
            .get(usize::try_from(index)?)
            .map(Vec::as_slice)
            .context("KV3 binary reference out of range")
    }
}
pub fn parse_binary(text: &str) -> Result<BinaryDocument> {
    let mut blobs = Vec::new();
    let value = parse_document(text, Some(&mut blobs), 256 * 1024 * 1024)?;
    Ok(BinaryDocument { value, blobs })
}
fn parse_document(text: &str, blobs: Option<&mut Vec<Vec<u8>>>, limit: usize) -> Result<Value> {
    ensure!(text.len() <= limit, "KV3 text exceeds size limit");
    let text = if let Some(header) = text.find("<!-- kv3 ") {
        let end = text[header..].find("-->").context("Truncated KV3 header")?;
        &text[header + end + 3..]
    } else {
        text
    };
    let mut parser = Parser {
        text,
        at: 0,
        nodes: 0,
        blobs,
    };
    let value = parser.value(0)?;
    parser.space()?;
    ensure!(parser.at == text.len(), "Unexpected trailing KV3 data");
    ensure!(value.is_object(), "Expected KV3 root object");
    Ok(value)
}

struct Parser<'a, 'b> {
    text: &'a str,
    at: usize,
    nodes: usize,
    blobs: Option<&'b mut Vec<Vec<u8>>>,
}

impl Parser<'_, '_> {
    fn space(&mut self) -> Result<()> {
        loop {
            while self.byte().is_some_and(u8::is_ascii_whitespace) {
                self.at += 1;
            }
            if self.text[self.at..].starts_with("//") {
                self.at += self.text[self.at..]
                    .find('\n')
                    .unwrap_or(self.text.len() - self.at);
            } else if self.text[self.at..].starts_with("/*") {
                let end = self.text[self.at + 2..]
                    .find("*/")
                    .context("Unterminated KV3 comment")?;
                self.at += end + 4;
            } else {
                return Ok(());
            }
        }
    }

    fn byte(&self) -> Option<&u8> {
        self.text.as_bytes().get(self.at)
    }

    fn take(&mut self, byte: u8) -> Result<()> {
        self.space()?;
        ensure!(
            self.byte() == Some(&byte),
            "Expected '{}' at KV3 byte {}",
            byte as char,
            self.at
        );
        self.at += 1;
        Ok(())
    }

    fn string(&mut self) -> Result<String> {
        // VRF writes embedded model key-values as KV3 raw multiline strings.
        if self.text[self.at..].starts_with("\"\"\"") {
            let start = self.at + 3;
            let end = self.text[start..]
                .find("\"\"\"")
                .context("Unterminated KV3 multiline string")?
                + start;
            self.at = end + 3;
            return Ok(self.text[start..end].to_owned());
        }
        let start = self.at;
        self.at += 1;
        let mut escaped = false;
        while let Some(&byte) = self.byte() {
            self.at += 1;
            if byte == b'"' && !escaped {
                return serde_json::from_str(&self.text[start..self.at])
                    .context("Invalid KV3 string");
            }
            escaped = byte == b'\\' && !escaped;
        }
        bail!("Unterminated KV3 string")
    }

    fn word(&mut self) -> Result<&str> {
        let start = self.at;
        while self
            .byte()
            .is_some_and(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'+'))
        {
            self.at += 1;
        }
        ensure!(self.at > start, "Expected KV3 token at byte {}", start);
        Ok(&self.text[start..self.at])
    }

    fn value(&mut self, depth: usize) -> Result<Value> {
        ensure!(depth <= 64, "KV3 nesting exceeds limit");
        self.nodes += 1;
        ensure!(self.nodes <= 4_000_000, "KV3 value count exceeds limit");
        self.space()?;
        match self.byte().copied().context("Truncated KV3 value")? {
            b'{' => {
                self.at += 1;
                let mut result = Map::new();
                loop {
                    self.space()?;
                    if self.byte() == Some(&b'}') {
                        self.at += 1;
                        break;
                    }
                    let key = if self.byte() == Some(&b'"') {
                        self.string()?
                    } else {
                        self.word()?.to_owned()
                    };
                    ensure!(self.blobs.is_none() || key != "$binary", "reserved KV3 binary key");
                    self.take(b'=')?;
                    ensure!(!result.contains_key(&key), "Duplicate KV3 key {key}");
                    result.insert(key, self.value(depth + 1)?);
                    self.space()?;
                    if self.byte() == Some(&b',') {
                        self.at += 1;
                    }
                }
                Ok(Value::Object(result))
            }
            b'[' => {
                self.at += 1;
                let mut values = Vec::new();
                loop {
                    self.space()?;
                    if self.byte() == Some(&b']') {
                        self.at += 1;
                        break;
                    }
                    values.push(self.value(depth + 1)?);
                    self.space()?;
                    if self.byte() == Some(&b',') {
                        self.at += 1;
                    }
                }
                Ok(Value::Array(values))
            }
            b'#' => {
                self.at += 1;
                self.take(b'[')?;
                let mut bytes = Vec::new();
                loop {
                    self.space()?;
                    if self.byte() == Some(&b']') {
                        self.at += 1;
                        break;
                    }
                    let token = self.word()?;
                    ensure!(
                        token.len() == 2 && token.bytes().all(|b| b.is_ascii_hexdigit()),
                        "Invalid KV3 binary byte"
                    );
                    bytes.push(u8::from_str_radix(token, 16)?);
                    ensure!(bytes.len() <= 16 * 1024 * 1024, "KV3 binary exceeds limit");
                }
                if let Some(blobs) = self.blobs.as_mut() {
                    let index = blobs.len();
                    blobs.push(bytes);
                    Ok(serde_json::json!({"$binary": index}))
                } else {
                    Ok(Value::Array(bytes.into_iter().map(Value::from).collect()))
                }
            }
            b'"' => Ok(Value::String(self.string()?)),
            _ => {
                let token = self.word()?.to_owned();
                self.space()?;
                if self.byte() == Some(&b':') {
                    ensure!(
                        matches!(
                            token.as_str(),
                            "resource" | "resource_name" | "subclass" | "soundevent"
                        ),
                        "Unsupported KV3 annotation {token}"
                    );
                    self.at += 1;
                    self.space()?;
                    ensure!(self.byte() == Some(&b'"'), "Expected annotated KV3 string");
                    return Ok(Value::String(self.string()?));
                }
                match token.as_str() {
                    "true" => Ok(Value::Bool(true)),
                    "false" => Ok(Value::Bool(false)),
                    "null" => Ok(Value::Null),
                    _ => {
                        let value: Value =
                            serde_json::from_str(&token).context("Invalid KV3 scalar")?;
                        ensure!(value.is_number(), "Expected KV3 number");
                        Ok(value)
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packed_binary_preserves_bytes_without_json_byte_arrays() {
        let text = "{ data=#[ 00 1f FF ] nested=[{data=#[ 02 03 ]}] }";
        let packed = parse_binary(text).unwrap();
        assert_eq!(packed.binary(&packed.value["data"]).unwrap(), &[0, 31, 255]);
        assert_eq!(
            packed.binary(&packed.value["nested"][0]["data"]).unwrap(),
            &[2, 3]
        );
        assert_eq!(
            parse(text).unwrap()["data"],
            serde_json::json!([0, 31, 255])
        );
        assert!(packed.binary(&serde_json::json!({"$binary":99})).is_err());
        assert!(parse_binary("{data=#[ FF}").is_err());
        assert!(parse_binary("{fake={ $binary=0 } data=#[ 00 ]}").is_err());
    }
    #[test]
    fn reads_embedded_model_multiline_kv3() {
        let value = parse(
            r#"{ text = """
{ quoted = "inner" }
""" refs = [resource:"animation/test.vnmskel"] }"#,
        )
        .unwrap();
        assert_eq!(value["text"], "\n{ quoted = \"inner\" }\n");
        assert_eq!(value["refs"][0], "animation/test.vnmskel");
        assert!(parse(r#"{ text = """unterminated }"#).is_err());
    }

    #[test]
    fn reads_resource_text_and_rejects_truncation() {
        let v = parse("preamble\n<!-- kv3 encoding:text -->\n{ path=resource:\"a\\\\b\" bytes=#[ 00 1f FF ] values=[1,-2,3.5e-2,true,] }").unwrap();
        assert_eq!(v["bytes"], serde_json::json!([0, 31, 255]));
        assert_eq!(v["path"], "a\\b");
        for bad in [
            "{a=#[ 0 ]}",
            "{a=[1,2}",
            "{a=1 a=2}",
            "{a=\"unterminated}",
            "{a=nan}",
            "{} garbage",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }
}
