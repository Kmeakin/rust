use crate::io;
use crate::iter::Iterator;
use crate::mem::MaybeUninit;
use crate::os::uefi;
use crate::ptr::NonNull;

pub struct Stdin {
    pending: Option<char>,
}

pub struct Stdout;
pub struct Stderr;

impl Stdin {
    pub const fn new() -> Stdin {
        Stdin { pending: None }
    }
}

impl io::Read for Stdin {
    fn read(&mut self, mut buf: &mut [u8]) -> io::Result<usize> {
        let st: NonNull<r_efi::efi::SystemTable> = uefi::env::system_table().cast();
        let stdin = unsafe { (*st.as_ptr()).con_in };

        // Write any pending character
        if let Some(ch) = self.pending {
            if ch.len_utf8() > buf.len() {
                return Ok(0);
            }
            ch.encode_utf8(buf);
            buf = &mut buf[ch.len_utf8()..];
            self.pending = None;
        }

        // Try reading any pending data
        let inp = read(stdin)?;

        // Check if the key is printiable character
        if inp == 0x00 {
            return Err(io::const_io_error!(io::ErrorKind::Interrupted, "Special Key Press"));
        }

        // The option unwrap is safe since iterator will have 1 element.
        let ch: char = char::decode_utf16([inp])
            .next()
            .unwrap()
            .map_err(|_| io::const_io_error!(io::ErrorKind::InvalidInput, "Invalid Input"))?;
        if ch.len_utf8() > buf.len() {
            self.pending = Some(ch);
            return Ok(0);
        }

        ch.encode_utf8(buf);

        Ok(ch.len_utf8())
    }
}

impl Stdout {
    pub const fn new() -> Stdout {
        Stdout
    }
}

impl io::Write for Stdout {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let st: NonNull<r_efi::efi::SystemTable> = uefi::env::system_table().cast();
        let stdout = unsafe { (*st.as_ptr()).con_out };

        write(stdout, buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Stderr {
    pub const fn new() -> Stderr {
        Stderr
    }
}

impl io::Write for Stderr {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let st: NonNull<r_efi::efi::SystemTable> = uefi::env::system_table().cast();
        let stderr = unsafe { (*st.as_ptr()).std_err };

        write(stderr, buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

// UCS-2 character should occupy 3 bytes at most in UTF-8
pub const STDIN_BUF_SIZE: usize = 3;

pub fn is_ebadf(err: &io::Error) -> bool {
    err.raw_os_error() == Some(r_efi::efi::Status::UNSUPPORTED.as_usize())
}

pub fn panic_output() -> Option<impl io::Write> {
    uefi::env::try_system_table().map(|_| Stderr::new())
}

fn write(
    protocol: *mut r_efi::protocols::simple_text_output::Protocol,
    buf: &[u8],
) -> io::Result<usize> {
    // Get valid UTF-8 buffer
    let utf8 = match crate::str::from_utf8(buf) {
        Ok(x) => x,
        Err(e) => unsafe { crate::str::from_utf8_unchecked(&buf[..e.valid_up_to()]) },
    };

    let mut utf16: Vec<u16> = utf8.encode_utf16().collect();
    utf16.push(0);

    unsafe { simple_text_output(protocol, &mut utf16) }?;

    Ok(utf8.len())
}

unsafe fn simple_text_output(
    protocol: *mut r_efi::protocols::simple_text_output::Protocol,
    buf: &mut [u16],
) -> io::Result<()> {
    let res = unsafe { ((*protocol).output_string)(protocol, buf.as_mut_ptr()) };
    if res.is_error() { Err(io::Error::from_raw_os_error(res.as_usize())) } else { Ok(()) }
}

fn read(stdin: *mut r_efi::protocols::simple_text_input::Protocol) -> io::Result<u16> {
    loop {
        match read_key_stroke(stdin) {
            Ok(x) => return Ok(x.unicode_char),
            Err(e) if e == r_efi::efi::Status::NOT_READY => wait_stdin(stdin)?,
            Err(e) => return Err(io::Error::from_raw_os_error(e.as_usize())),
        }
    }
}

fn wait_stdin(stdin: *mut r_efi::protocols::simple_text_input::Protocol) -> io::Result<()> {
    let boot_services: NonNull<r_efi::efi::BootServices> =
        uefi::env::boot_services().unwrap().cast();
    let wait_for_event = unsafe { (*boot_services.as_ptr()).wait_for_event };
    let wait_for_key_event = unsafe { (*stdin).wait_for_key };

    let r = {
        let mut x: usize = 0;
        (wait_for_event)(1, [wait_for_key_event].as_mut_ptr(), &mut x)
    };
    if r.is_error() { Err(io::Error::from_raw_os_error(r.as_usize())) } else { Ok(()) }
}

fn read_key_stroke(
    stdin: *mut r_efi::protocols::simple_text_input::Protocol,
) -> Result<r_efi::protocols::simple_text_input::InputKey, r_efi::efi::Status> {
    let mut input_key: MaybeUninit<r_efi::protocols::simple_text_input::InputKey> =
        MaybeUninit::uninit();

    let r = unsafe { ((*stdin).read_key_stroke)(stdin, input_key.as_mut_ptr()) };

    if r.is_error() {
        Err(r)
    } else {
        let input_key = unsafe { input_key.assume_init() };
        Ok(input_key)
    }
}
