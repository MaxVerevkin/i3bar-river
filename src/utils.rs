//! Some usefull functions

use std::io;
use std::os::fd::{AsFd, AsRawFd};

use serde::Deserialize;
use serde_json::{Deserializer, Error as JsonError};

/// Read from a raw file descriptor to the vector.
///
/// Appends data at the end of the buffer. Resizes vector as needed.
pub fn read_to_vec(fd: impl AsFd, buf: &mut Vec<u8>) -> io::Result<usize> {
    buf.reserve(1024);

    let res = unsafe {
        libc::read(
            fd.as_fd().as_raw_fd(),
            buf.as_mut_ptr().add(buf.len()).cast(),
            (buf.capacity() - buf.len()) as libc::size_t,
        )
    };

    if res == -1 {
        return Err(io::Error::last_os_error());
    }

    let read = res as usize;
    unsafe { buf.set_len(buf.len() + read) };

    Ok(read)
}

/// Retuns (`last_line`, `remaining`). See tests for examples.
pub fn last_line(s: &[u8]) -> Option<(&[u8], &[u8])> {
    let mut it = memchr::memrchr_iter(b'\n', s);
    let last = it.next()?;
    let rem = &s[(last + 1)..];
    if let Some(pre_last) = it.next() {
        Some((&s[(pre_last + 1)..last], rem))
    } else {
        Some((&s[..last], rem))
    }
}

/// Deserialize the last complete object. Returns (`object`, `remaining`). See tests for examples.
pub fn de_last_json<'a, T: Deserialize<'a>>(
    mut s: &'a [u8],
) -> Result<(Option<T>, &'a [u8]), JsonError> {
    let mut last = None;
    let mut tmp;
    loop {
        (tmp, s) = de_first_json(s)?;
        last = match tmp {
            Some(obj) => Some(obj),
            None => return Ok((last, s)),
        };
    }
}

/// Deserialize the first complete object. Returns (`object`, `remaining`). See tests for examples.
pub fn de_first_json<'a, T: Deserialize<'a>>(
    mut s: &'a [u8],
) -> Result<(Option<T>, &'a [u8]), JsonError> {
    while s
        .first()
        .map_or(false, |&x| x == b' ' || x == b',' || x == b'\n')
    {
        s = &s[1..];
    }
    let mut de = Deserializer::from_slice(s).into_iter();
    match de.next() {
        Some(Ok(obj)) => Ok((Some(obj), &s[de.byte_offset()..])),
        Some(Err(e)) if e.is_eof() => Ok((None, &s[de.byte_offset()..])),
        Some(Err(e)) => Err(e),
        None => Ok((None, &s[de.byte_offset()..])),
    }
}

/// Returns a byte slice with leading ASCII whitespace bytes removed.
///
/// TODO: remove if/when slice::trim_ascii_start is stabilized.
pub fn trim_ascii_start(mut bytes: &[u8]) -> &[u8] {
    while let [first, rest @ ..] = bytes {
        if first.is_ascii_whitespace() {
            bytes = rest;
        } else {
            break;
        }
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! str {
        ($str:expr) => {
            &$str.as_bytes()[..]
        };
    }

    #[test]
    fn streaming_json() {
        let s = b",[2]\n, [3], [4, 3],[32][3] ";
        assert_eq!(
            de_first_json::<Vec<u8>>(s).unwrap(),
            (Some(vec![2]), str!("\n, [3], [4, 3],[32][3] "))
        );
        assert_eq!(
            de_last_json::<Vec<u8>>(s).unwrap(),
            (Some(vec![3]), str!(""))
        );

        let s = b",[2]\n, [3], [4, 3],[32][3] [2, 4";
        assert_eq!(
            de_last_json::<Vec<u8>>(s).unwrap(),
            (Some(vec![3]), str!("[2, 4"))
        );

        let s = b",[2]\n, [3], [4, 3],[32] invalid";
        assert_eq!(
            de_first_json::<Vec<u8>>(s).unwrap(),
            (Some(vec![2]), str!("\n, [3], [4, 3],[32] invalid"))
        );
        assert!(de_last_json::<Vec<u8>>(s).is_err());
    }

    #[test]
    fn test_last_line() {
        let s = b"hello";
        assert_eq!(last_line(s), None);

        let s = b"hello\n";
        assert_eq!(last_line(s), Some((str!("hello"), str!(""))));

        let s = b"hello\nworld";
        assert_eq!(last_line(s), Some((str!("hello"), str!("world"))));

        let s = b"hello\nworld\n";
        assert_eq!(last_line(s), Some((str!("world"), str!(""))));

        let s = b"hello\nworld\n...";
        assert_eq!(last_line(s), Some((str!("world"), str!("..."))));
    }

    #[test]
    fn test_trim_start() {
        let s = b" ";
        assert_eq!(trim_ascii_start(s), b"");

        let s = b"hello";
        assert_eq!(trim_ascii_start(s), b"hello");

        let s = b"\t \nhello";
        assert_eq!(trim_ascii_start(s), b"hello");

        let s = b" \t \nhello\n";
        assert_eq!(trim_ascii_start(s), b"hello\n");
    }
}
