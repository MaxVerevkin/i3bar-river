use std::borrow::Cow;
use std::io::{BufRead, BufReader, Read, Result};

pub struct LinesBuffer<R> {
    reader: BufReader<R>,
    next_line: Vec<u8>,
}

impl<R: Read> LinesBuffer<R> {
    pub fn new(inner: R) -> Self {
        Self {
            reader: BufReader::new(inner),
            next_line: Vec::new(),
        }
    }

    pub fn fill_buf(&mut self) -> Result<()> {
        self.reader.fill_buf()?;
        Ok(())
    }

    pub fn inner(&self) -> &R {
        self.reader.get_ref()
    }
}

impl<R: Read> Iterator for &mut LinesBuffer<R> {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        let pending = self.reader.buffer();
        let (result, used) = match memchr::memchr(b'\n', pending) {
            None => {
                self.next_line.extend_from_slice(pending);
                (None, pending.len())
            }
            Some(i) => {
                self.next_line.extend_from_slice(&pending[..i]);
                let line = from_utf8(&self.next_line);
                self.next_line.clear();
                (Some(line), i + 1)
            }
        };
        self.reader.consume(used);
        result
    }
}

fn from_utf8(bytes: &[u8]) -> String {
    match String::from_utf8_lossy(bytes) {
        Cow::Borrowed(s) => s.to_owned(),
        Cow::Owned(s) => s,
    }
}
