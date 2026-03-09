pub fn escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

pub fn string(value: &str) -> String {
    format!("\"{}\"", escape(value))
}

pub fn bool_value(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

pub fn array(items: &[String]) -> String {
    format!("[{}]", items.join(","))
}

pub fn object(fields: &[(&str, String)]) -> String {
    let content = fields
        .iter()
        .map(|(key, value)| format!("{}:{}", string(key), value))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{{content}}}")
}

pub fn encode_field(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'%' | b'\n' | b'\r' | b'\t' | b'|' => {
                encoded.push('%');
                encoded.push_str(&format!("{byte:02X}"));
            }
            _ => encoded.push(char::from(byte)),
        }
    }
    encoded
}

pub fn decode_field(value: &str) -> Option<String> {
    let mut decoded = String::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return None;
            }
            let segment = std::str::from_utf8(&bytes[index + 1..index + 3]).ok()?;
            let parsed = u8::from_str_radix(segment, 16).ok()?;
            decoded.push(char::from(parsed));
            index += 3;
        } else {
            decoded.push(char::from(bytes[index]));
            index += 1;
        }
    }
    Some(decoded)
}
