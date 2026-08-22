#[cfg(debug_assertions)]
use std::fs::OpenOptions;
#[cfg(debug_assertions)]
use std::io::Write;

pub const LOG_FILE: &str = "/tmp/nxvim.log";


#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {
        $crate::write_log(format_args!($($arg)*));
    };
}

pub fn write_log(args: std::fmt::Arguments) {
    #[cfg(debug_assertions)]
    {
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(LOG_FILE)
        {
            let _ = writeln!(file, "{}", args);
        }
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = args;
    }
}

