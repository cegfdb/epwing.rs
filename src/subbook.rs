use std;
use std::io::{Reader, Seek, SeekSet, IoResult, IoError};
use std::error::FromError;
use jis0208;
use unicode;

#[deriving(Show)]
struct IndexLocation {
    page: u32,
    length: u32
}

#[deriving(Show)]
struct Indices {
    menu: Option<IndexLocation>,
    copyright: Option<IndexLocation>,
}

pub struct Subbook<IO> {
    io: IO,
    indices: Indices
}

impl<IO> std::fmt::Show for Subbook<IO> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Subbook {{ io: ..., indices: {} }}", self.indices)
    }
}

#[deriving(Show)]
pub enum Error {
    Io(IoError),
    InvalidEncoding,
    InvalidControlCode(u8)
}

impl FromError<IoError> for Error {
    fn from_error(err: IoError) -> Error {
        Io(err)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

impl<IO: Reader+Seek> Subbook<IO> {
    pub fn from_io(mut io: IO) -> Result<Subbook<IO>> {
        let indices = try!(Indices::read_from(&mut io));

        Ok(Subbook {
            io: io,
            indices: indices
        })
    }

    pub fn read_text(&mut self, page: u32, offset: u16) -> Result<Text> {
        try!(self.io.seek( ((page - 1) * 0x800 + offset as u32) as i64, SeekSet ));
        read_text(&mut self.io)
    }
}

impl Indices {
    fn read_from<R: Reader+Seek>(io: &mut R) -> IoResult<Indices> {
        try!(io.seek(1, SeekSet));
        let n_indices = try!(io.read_u8());

        try!(io.seek(4, SeekSet));
        let global_avail = try!(io.read_u8());

        let mut ics = Indices {
            menu: None, copyright: None
        };

        for i in range(0, n_indices) {
            try!(io.seek((16 + i * 16) as i64, SeekSet));

            let index_id = try!(io.read_u8());
            try!(io.read_exact(1));
            let start_page = try!(io.read_be_u32());
            let page_count = try!(io.read_be_u32());
            let avail = try!(io.read_u8());

            let loc = IndexLocation { page: start_page, length: page_count };

            match index_id {
                0x01 => ics.menu = Some(loc),
                0x02 => ics.copyright = Some(loc),
                _ => ()
            }
        }

        Ok(ics)
    }
}

#[deriving(Show)]
pub enum TextElement {
    UnicodeString(String),
    CustomCharacter(u16),
    Newline
}

#[deriving(Show)]
pub struct Text(Vec<TextElement>);

fn read_text<R: Reader+Seek>(io: &mut R) -> Result<Text> {
    let mut text = Vec::new();

    let started_at = try!(io.tell());
    let mut is_narrow = false;
    let mut delimiter_keyword = None;

    loop {
        let byte = try!(io.read_u8());
        match byte {
            0x1f => {
                match try!(io.read_u8()) {
                    // Start text
                    0x02 => (),
                    // End text
                    0x03 => break,
                    // Start narrow text
                    0x04 => is_narrow = true,
                    // End narrow text
                    0x05 => is_narrow = false,
                    // Newline
                    0x0a => text.push(Newline),
                    // Begin keyword
                    0x41 => {
                        let keyword = try!(io.read_be_u16());
                        if delimiter_keyword == Some(keyword) {
                            // Next entry encountered, stop.
                            break;
                        } else if delimiter_keyword.is_none() {
                            delimiter_keyword = Some(keyword);
                        }
                    },
                    // End keyword
                    0x61 => (),

                    cc => return Err(InvalidControlCode(cc))
                }
            },
            _ => {
                let other = try!(io.read_u8());
                let codepoint = (byte as u16 << 8) | (other as u16);

                if let Some(mut ch) = jis0208::decode_codepoint(codepoint) {
                    if is_narrow {
                        if let Some(2) = unicode::char::width(ch, true) {
                            /* FIXME
                             * Using a decomposition might affect other characters than the ones we
                             * want. Use a proper table.
                             */
                            unicode::char::decompose_compatible(ch, |new_ch| ch = new_ch);
                        }
                    }
                    if let Some(&UnicodeString(ref mut s)) = text.last_mut() {
                        s.push(ch);
                    } else {
                        text.push(UnicodeString(String::from_char(1, ch)));
                    }
                } else {
                    text.push(CustomCharacter(codepoint));
                }
            }
        }
    }

    Ok(Text(text))
}

impl Text {
    pub fn to_plaintext(&self) -> String {
        let mut out = String::new();

        let Text(ref elems) = *self;

        for elem in elems.iter() {
            match *elem {
                UnicodeString(ref s) => out.push_str(s.as_slice()),
                CustomCharacter(cp) => (),
                Newline => out.push('\n')
            }
        }

        out
    }
}
