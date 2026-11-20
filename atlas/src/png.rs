//! Writes a PNG without any library: RGBA, eight bits a channel, compressed with a small
//! deflate of its own (fixed Huffman codes, matches found through a hash of four bytes).
//! A map is mostly runs and repeats, which this packs to a few percent. The picture is given
//! in strips, top first, so an export can be spread over many frames.

const WINDOW: usize = 32 * 1024;
const HASH_BITS: u32 = 15;

/// CRC-32 as PNG chunks carry it.
fn crc32(seed: u32, bytes: &[u8]) -> u32 {
    let mut crc = !seed;
    for b in bytes {
        crc ^= u32::from(*b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { crc >> 1 ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

struct Bits {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl Bits {
    /// `count` bits of `value`, lowest first.
    #[inline]
    fn put(&mut self, value: u32, count: u32) {
        self.acc |= u64::from(value) << self.n;
        self.n += count;
        while self.n >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }

    /// A Huffman code: `count` bits, highest first.
    #[inline]
    fn code(&mut self, code: u32, count: u32) {
        self.put(code.reverse_bits() >> (32 - count), count);
    }

    /// A literal or length symbol in deflate's fixed code.
    #[inline]
    fn symbol(&mut self, s: u32) {
        match s {
            0..=143 => self.code(0x30 + s, 8),
            144..=255 => self.code(0x190 + s - 144, 9),
            256..=279 => self.code(s - 256, 7),
            _ => self.code(0xC0 + s - 280, 8),
        }
    }

    fn finish_byte(&mut self) {
        if self.n > 0 {
            self.out.push(self.acc as u8);
            self.acc = 0;
            self.n = 0;
        }
    }
}

const LENGTH_BASE: [u16; 29] =
    [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LENGTH_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145,
    8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] =
    [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

/// One deflate block of `data` in the fixed code, not the last.
fn deflate_block(bits: &mut Bits, data: &[u8], table: &mut [u32]) {
    bits.put(0, 1);
    bits.put(1, 2);
    table.fill(u32::MAX);
    let hash = |at: usize| {
        let v = u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
        (v.wrapping_mul(0x9E37_79B1) >> (32 - HASH_BITS)) as usize
    };
    let mut i = 0;
    while i < data.len() {
        let mut matched = 0;
        let mut distance = 0;
        if i + 4 <= data.len() {
            let h = hash(i);
            let candidate = table[h] as usize;
            table[h] = i as u32;
            if candidate != u32::MAX as usize && i - candidate <= WINDOW {
                let most = (data.len() - i).min(258);
                while matched < most && data[candidate + matched] == data[i + matched] {
                    matched += 1;
                }
                distance = i - candidate;
            }
        }
        if matched < 4 {
            bits.symbol(u32::from(data[i]));
            i += 1;
            continue;
        }
        let l = LENGTH_BASE.iter().rposition(|b| usize::from(*b) <= matched).unwrap_or(0);
        bits.symbol(257 + l as u32);
        bits.put((matched - usize::from(LENGTH_BASE[l])) as u32, u32::from(LENGTH_EXTRA[l]));
        let d = DIST_BASE.iter().rposition(|b| usize::from(*b) <= distance).unwrap_or(0);
        bits.code(d as u32, 5);
        bits.put((distance - usize::from(DIST_BASE[d])) as u32, u32::from(DIST_EXTRA[d]));
        // The positions inside the match are found again later through its first bytes.
        let end = i + matched;
        i += 1;
        while i < end && i + 4 <= data.len() {
            table[hash(i)] = i as u32;
            i += 2;
        }
        i = end;
    }
    bits.symbol(256);
}

/// A PNG being written.
pub struct Png {
    width: usize,
    out: Vec<u8>,
    bits: Bits,
    table: Vec<u32>,
    /// The rows of a strip with their filter bytes.
    raw: Vec<u8>,
    adler: (u32, u32),
    rows: usize,
}

impl Png {
    pub fn new(width: usize, height: usize) -> Self {
        let mut out = Vec::with_capacity(1 << 16);
        out.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        let mut header = Vec::with_capacity(13);
        header.extend_from_slice(&(width as u32).to_be_bytes());
        header.extend_from_slice(&(height as u32).to_be_bytes());
        // Eight bits, RGBA, deflate, no filter method choice, not interlaced.
        header.extend_from_slice(&[8, 6, 0, 0, 0]);
        chunk(&mut out, b"IHDR", &header);
        let mut bits = Bits { out: Vec::new(), acc: 0, n: 0 };
        // The zlib header: deflate, 32K window, fastest.
        bits.out.extend_from_slice(&[0x78, 0x01]);
        Self {
            width,
            out,
            bits,
            table: vec![u32::MAX; 1 << HASH_BITS],
            raw: Vec::new(),
            adler: (1, 0),
            rows: 0,
        }
    }

    /// The bytes written so far.
    pub fn len(&self) -> usize {
        self.out.len()
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Adds rows of RGBA pixels, `width` across, top first.
    pub fn strip(&mut self, rgba: &[u8]) {
        let stride = self.width * 4;
        self.raw.clear();
        for row in rgba.chunks_exact(stride) {
            // Filter 0: the bytes as they are.
            self.raw.push(0);
            self.raw.extend_from_slice(row);
            self.rows += 1;
        }
        for part in self.raw.chunks(5552) {
            for b in part {
                self.adler.0 += u32::from(*b);
                self.adler.1 += self.adler.0;
            }
            self.adler.0 %= 65521;
            self.adler.1 %= 65521;
        }
        deflate_block(&mut self.bits, &self.raw, &mut self.table);
        if !self.bits.out.is_empty() {
            chunk(&mut self.out, b"IDAT", &self.bits.out);
            self.bits.out.clear();
        }
    }

    /// Ends the picture and gives the file.
    pub fn finish(mut self) -> Vec<u8> {
        // A last, empty block, then the checksum of everything that went in.
        self.bits.put(1, 1);
        self.bits.put(1, 2);
        self.bits.symbol(256);
        self.bits.finish_byte();
        self.bits.out.extend_from_slice(&(self.adler.1 << 16 | self.adler.0).to_be_bytes());
        chunk(&mut self.out, b"IDAT", &self.bits.out);
        chunk(&mut self.out, b"IEND", &[]);
        self.out
    }
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], body: &[u8]) {
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(body);
    out.extend_from_slice(&crc32(crc32(0, kind), body).to_be_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plain inflate for the fixed code, to read back what `deflate_block` wrote.
    fn inflate(data: &[u8]) -> Vec<u8> {
        struct Reader<'a> {
            data: &'a [u8],
            at: usize,
        }
        impl Reader<'_> {
            fn bit(&mut self) -> u32 {
                let b = u32::from(self.data[self.at / 8] >> (self.at % 8) & 1);
                self.at += 1;
                b
            }
            fn bits(&mut self, n: u32) -> u32 {
                (0..n).fold(0, |v, i| v | self.bit() << i)
            }
            fn code(&mut self, n: u32) -> u32 {
                (0..n).fold(0, |v, _| v << 1 | self.bit())
            }
        }
        let mut r = Reader { data, at: 0 };
        let mut out = Vec::new();
        loop {
            let last = r.bits(1);
            assert_eq!(r.bits(2), 1, "fixed code");
            loop {
                let mut c = r.code(7);
                let symbol = if c <= 0x17 {
                    c + 256
                } else {
                    c = c << 1 | r.bit();
                    if (0x30..=0xBF).contains(&c) {
                        c - 0x30
                    } else if (0xC0..=0xC7).contains(&c) {
                        c - 0xC0 + 280
                    } else {
                        (c << 1 | r.bit()) - 0x190 + 144
                    }
                };
                if symbol < 256 {
                    out.push(symbol as u8);
                } else if symbol == 256 {
                    break;
                } else {
                    let l = (symbol - 257) as usize;
                    let len = usize::from(LENGTH_BASE[l]) + r.bits(u32::from(LENGTH_EXTRA[l])) as usize;
                    let d = r.code(5) as usize;
                    let dist = usize::from(DIST_BASE[d]) + r.bits(u32::from(DIST_EXTRA[d])) as usize;
                    for _ in 0..len {
                        out.push(out[out.len() - dist]);
                    }
                }
            }
            if last == 1 {
                return out;
            }
        }
    }

    #[test]
    fn a_picture_reads_back() {
        let (w, h) = (300usize, 70usize);
        let mut pixels = vec![0u8; w * h * 4];
        for (i, p) in pixels.chunks_exact_mut(4).enumerate() {
            let (x, y) = (i % w, i / w);
            // Runs, a dither and some noise.
            let v = if y < 20 { 80 } else if (x + y) % 2 == 0 { 120 } else { (x * 7 + y * 13) as u8 };
            p.copy_from_slice(&[v, v / 2, 255 - v, if x < 10 { 0 } else { 255 }]);
        }
        let mut png = Png::new(w, h);
        png.strip(&pixels[..w * 4 * 32]);
        png.strip(&pixels[w * 4 * 32..]);
        assert_eq!(png.rows(), h);
        let file = png.finish();
        assert_eq!(&file[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        assert!(file.len() < w * h * 4 / 2, "{}", file.len());
        // Walk the chunks: checksums hold, and the data inflates to the rows.
        let (mut at, mut data, mut kinds) = (8, Vec::new(), Vec::new());
        while at < file.len() {
            let len = u32::from_be_bytes(file[at..at + 4].try_into().unwrap()) as usize;
            let kind = &file[at + 4..at + 8];
            let body = &file[at + 8..at + 8 + len];
            let crc = u32::from_be_bytes(file[at + 8 + len..at + 12 + len].try_into().unwrap());
            assert_eq!(crc, crc32(crc32(0, kind), body));
            if kind == b"IDAT" {
                data.extend_from_slice(body);
            }
            kinds.push(String::from_utf8_lossy(kind).into_owned());
            at += 12 + len;
        }
        assert_eq!(kinds.first().map(String::as_str), Some("IHDR"));
        assert_eq!(kinds.last().map(String::as_str), Some("IEND"));
        assert_eq!(&data[..2], &[0x78, 0x01]);
        let raw = inflate(&data[2..data.len() - 4]);
        assert_eq!(raw.len(), (w * 4 + 1) * h);
        for (y, row) in raw.chunks_exact(w * 4 + 1).enumerate() {
            assert_eq!(row[0], 0);
            assert_eq!(&row[1..], &pixels[y * w * 4..(y + 1) * w * 4], "row {y}");
        }
        let (mut a, mut b) = (1u32, 0u32);
        for byte in &raw {
            a = (a + u32::from(*byte)) % 65521;
            b = (b + a) % 65521;
        }
        assert_eq!(&data[data.len() - 4..], &(b << 16 | a).to_be_bytes());
        assert_eq!(crc32(0, b"123456789"), 0xCBF4_3926);
    }
}
