#[derive(Debug, Clone, PartialEq)]
pub enum WireValue {
    Varint(u64),
    Fixed32(u32),
    Fixed64(u64),
    Bytes(Vec<u8>),
}

impl WireValue {
    pub fn as_varint(&self) -> Option<u64> {
        match self {
            WireValue::Varint(v) => Some(*v),
            _ => None,
        }
    }
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            WireValue::Bytes(b) => Some(b),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<String> {
        self.as_bytes()
            .map(|b| String::from_utf8_lossy(b).into_owned())
    }
    pub fn as_f32(&self) -> Option<f32> {
        match self {
            WireValue::Fixed32(bits) => Some(f32::from_bits(*bits)),
            _ => None,
        }
    }
    pub fn as_i32(&self) -> Option<i32> {
        self.as_varint().map(|v| v as i32)
    }
}

pub fn decode_fields(data: &[u8]) -> Vec<(u32, WireValue)> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < data.len() {
        let (tag, next) = match read_varint(data, pos) {
            Ok(v) => v,
            Err(_) => break,
        };
        pos = next;
        let field = (tag >> 3) as u32;
        let wt = (tag & 7) as u8;
        match wt {
            0 => match read_varint(data, pos) {
                Ok((v, n)) => {
                    out.push((field, WireValue::Varint(v)));
                    pos = n;
                }
                Err(_) => break,
            },
            1 => {
                if pos + 8 > data.len() {
                    break;
                }
                let bits = u64::from_le_bytes(data[pos..pos + 8].try_into().expect("8"));
                out.push((field, WireValue::Fixed64(bits)));
                pos += 8;
            }
            2 => match read_varint(data, pos) {
                Ok((len, n)) => {
                    let len = len as usize;
                    if n + len > data.len() {
                        break;
                    }
                    out.push((field, WireValue::Bytes(data[n..n + len].to_vec())));
                    pos = n + len;
                }
                Err(_) => break,
            },
            5 => {
                if pos + 4 > data.len() {
                    break;
                }
                let bits = u32::from_le_bytes(data[pos..pos + 4].try_into().expect("4"));
                out.push((field, WireValue::Fixed32(bits)));
                pos += 4;
            }
            _ => break,
        }
    }
    out
}

fn read_varint(data: &[u8], mut pos: usize) -> Result<(u64, usize), ()> {
    let mut result = 0u64;
    let mut shift = 0u32;
    while pos < data.len() && shift < 70 {
        let b = data[pos];
        result |= ((b & 0x7f) as u64) << shift;
        pos += 1;
        if b & 0x80 == 0 {
            return Ok((result, pos));
        }
        shift += 7;
    }
    Err(())
}

#[derive(Default)]
pub struct PbWriter {
    pub buf: Vec<u8>,
}

impl PbWriter {
    fn tag(&mut self, field: u32, wt: u8) {
        put_varint(&mut self.buf, ((field as u64) << 3) | wt as u64);
    }
    pub fn varint(&mut self, field: u32, v: u64) {
        self.tag(field, 0);
        put_varint(&mut self.buf, v);
    }
    pub fn bytes(&mut self, field: u32, b: &[u8]) {
        self.tag(field, 2);
        put_varint(&mut self.buf, b.len() as u64);
        self.buf.extend_from_slice(b);
    }
    pub fn fixed32(&mut self, field: u32, v: u32) {
        self.tag(field, 5);
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn f32(&mut self, field: u32, v: f32) {
        self.fixed32(field, v.to_bits());
    }

    pub fn nested<F: FnOnce(&mut PbWriter)>(&mut self, field: u32, build: F) {
        let mut inner = PbWriter::default();
        build(&mut inner);
        self.bytes(field, &inner.buf);
    }
}

pub fn put_varint(buf: &mut Vec<u8>, mut v: u64) {
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            buf.push(b);
            break;
        }
        buf.push(b | 0x80);
    }
}
