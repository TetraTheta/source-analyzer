use std::fmt;

use anstyle::{AnsiColor, Style};

pub const EMBEDDED: Style = AnsiColor::Cyan.on_default();
pub const ERROR: Style = AnsiColor::Red.on_default().bold();
pub const INFO: Style = AnsiColor::Cyan.on_default();
pub const PRESENT: Style = AnsiColor::Green.on_default();
pub const WARNING: Style = AnsiColor::Yellow.on_default();

pub fn error(message: fmt::Arguments<'_>) {
  anstream::eprintln!("{ERROR}error:{ERROR:#} {message}");
}

pub fn info(message: fmt::Arguments<'_>) {
  anstream::eprintln!("{INFO}{message}{INFO:#}");
}

pub fn warning(message: fmt::Arguments<'_>) {
  anstream::eprintln!("{WARNING}{message}{WARNING:#}");
}
