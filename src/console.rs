use core::fmt;

pub struct SbiConsole;

impl fmt::Write for SbiConsole {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            if sbi_rt::console_write_byte(b).is_err() {
                return Err(fmt::Error);
            }
        }

        Ok(())
    }
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        _ = write!($crate::console::SbiConsole, $($arg)*);
    }};
}

#[macro_export]
macro_rules! println {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        _ = writeln!($crate::console::SbiConsole, $($arg)*);
    }};
}
