//! # Safe wrapper for lvm2cmd.h bindings of the crate.
//! TODO: moar detail

use std::{
    ffi::{c_char, c_int, c_void, CStr, CString, NulError},
    str::FromStr,
    sync::{
        mpsc::{self, Receiver, Sender},
        Condvar, LazyLock, Mutex,
    },
};

use crate::{lvm2_exit, lvm2_init, lvm2_log_fn, lvm2_run};

// addition to every command issued
const DEFAULT_LVM_FLAGS: &str = "--reportformat json";

/// Singletone to sync calls to LVM. Experiments showed that Lvm::new() may obtain the same handler leading to double-free, access-after-free, etc
static LVM: LazyLock<Mutex<Result<Lvm, CommandRetCode>>> = LazyLock::new(|| Mutex::new(Lvm::new()));

// Channel to get data from the logs
// LVM waits for the data on mutex + condvar
// log recorder searches for commands and pushes them to the channel
static CHANNEL: LazyLock<Mutex<(Sender<String>, Receiver<String>)>> =
    LazyLock::new(|| Mutex::new(mpsc::channel()));
static DATA_ARRIVED: Condvar = Condvar::new();
static CAPTURED_CMD_DATA: Mutex<String> = Mutex::new(String::new()); // whether special log line arrived that command is executing

/// LVM handle holder
pub struct Lvm {
    handle: Box<c_void>,
}

// unsafe impl Sync for Lvm {}

impl Lvm {
    /// # Acquire global LVM singleton and run the specified function
    /// It's a building block to run commands. Lazy init happens here and all relevant errors handling
    /// # Panics
    /// 1. same as Mutex::lock()
    /// 2. if closure panics
    /// # Error
    /// There are 3 cases when this function could return error:
    /// - CommandRetCode::InitFailed          - Lvm lazy init failed on first access
    /// - CommandRetCode::GlobalStatePoisoned - Mutex holding the global Lvm handler is poisoned (another thread panicked holding the lock => within this function)
    /// - other CommandRetCode - inner function returned Err(CommandRetCode)
    ///
    /// Inner function isn't supposed to return CommandRetCode directly.
    pub fn acquire_and<F: FnOnce(&mut Lvm) -> Result<String, CommandRetCode>>(
        f: F,
    ) -> Result<String, CommandRetCode> {
        let mut guard = match LVM.lock() {
            Ok(g) => g,
            Err(_e) => return Err(CommandRetCode::GlobalStatePoisoned),
        };

        match guard.as_mut() {
            Ok(lvm) => f(lvm),
            Err(_e) => Err(CommandRetCode::InitFailed), // hardocde to avoid ambiguety
        }
    }

    /// Initialize Lvm with handler
    fn new() -> Result<Self, CommandRetCode> {
        let handle = unsafe { lvm2_init() };
        // SAFETY: LVM subsystem allocates the structure
        unsafe {
            lvm2_log_fn(Some(log_capturer));
            handle
                .as_mut() // check for NULL, peeked here: https://gitlab.com/lvmteam/lvm2/-/blob/main/tools/lvmcmdlib.c#L34
                .map(|_| Self {
                    handle: Box::from_raw(handle),
                })
                .ok_or(CommandRetCode::InitFailed)
        }
    }

    /// Run LVM command through a global hander
    /// See `man 8 lvm` for list of available commands
    pub fn run(command: &str) -> Result<String, CommandRetCode> {
        Self::acquire_and(|lvm| lvm._run(format!("{command} {DEFAULT_LVM_FLAGS}")))
    }

    /// internal command runner
    fn _run(&mut self, command: String) -> Result<String, CommandRetCode> {
        let cmd = CString::from_str(command.as_str())
            .map_err(|e| CommandRetCode::InvalidCommandLine(e))?;
        match CommandRetCode::from(unsafe {
            lvm2_run(self.handle.as_mut(), cmd.as_c_str().as_ptr())
        }) {
            CommandRetCode::CommandSucceeded => (),
            other => return Err(other),
        }
        // receive data from logs
        let ch = CHANNEL
            .lock()
            .map_err(|_e| CommandRetCode::GlobalStatePoisoned)?;
        match ch.1.try_recv() {
            Ok(res) => Ok(res), // TODO: parse json
            Err(_) => match DATA_ARRIVED.wait(ch) {
                Ok(ch) => Ok(ch.1.recv().unwrap()), // UNWRAP: the other side cannot be closed - SENDER is static
                Err(_) => Err(CommandRetCode::GlobalStatePoisoned),
            },
        }
    }
}

impl Drop for Lvm {
    fn drop(&mut self) {
        unsafe {
            lvm2_exit(self.handle.as_mut());
        }
    }
}

/// Possible commands return codes
#[derive(Debug)]
pub enum CommandRetCode {
    // from lvm2cmd.h
    CommandSucceeded,
    NoSuchCommand,
    InvalidParameters,
    InitFailed,
    ProcessingFailed,

    // rust-specific "Codes"
    /// Command line contains \0 in the middle
    InvalidCommandLine(NulError),
    /// Global object is poisoned - some thread panic-ed on Lvm execution
    GlobalStatePoisoned,
    /// Channel to get data from logs is poisoned - some thread panic-ed on data send / receive
    DataChannelPoisoned,
    Unknown(i32),
}

impl From<i32> for CommandRetCode {
    fn from(v: i32) -> Self {
        match v {
            1 => Self::CommandSucceeded,
            2 => Self::NoSuchCommand,
            3 => Self::InvalidParameters,
            4 => Self::InitFailed,
            5 => Self::ProcessingFailed,
            v => Self::Unknown(v),
        }
    }
}

#[derive(Debug)]
enum LogLevel {
    FATAL,
    ERROR,
    PRINT,
    VERBOSE,
    VERY_VERBOSE,
    DEBUG,
    UNKNOWN, // exists only in rust
}

impl From<c_int> for LogLevel {
    fn from(value: c_int) -> Self {
        match value {
            2 => Self::FATAL,
            3 => Self::ERROR,
            4 => Self::PRINT,
            5 => Self::VERBOSE,
            6 => Self::VERY_VERBOSE,
            7 => Self::DEBUG,
            _ => Self::UNKNOWN,
        }
    }
}

/// callback for LVM logs
/// is used to capture commands execution results
/// ASSUMPTION: the underlying code guarantees that this fn gets called sequentially for sequential lines
///             JSON will be incorrect otherwise
extern "C" fn log_capturer(
    level: c_int,
    file: *const c_char,
    _line: c_int,
    _dm_errno: c_int,
    message: *const c_char,
) {
    let mut cmd_output = CAPTURED_CMD_DATA.lock().unwrap(); // UNWRAP: this mutex panics if this method panics => can't rely on logs piping anymore

    let message = unsafe { CStr::from_ptr(message) }.to_str().unwrap();
    let file = unsafe { CStr::from_ptr(file) }.to_str().unwrap();
    let level = LogLevel::from(level);
    match (&level, file) {
        (LogLevel::DEBUG, "lvmcmdline.c") => {
            if message.starts_with("Completed:") {
                // DEBUG "lvmcmdline.c" 3325 0 "Completed: pvs --reportformat json"
                if cmd_output.is_empty() {
                    // no PRINT messages
                    cmd_output.push_str(r#"{"rust_logger": "no messages from command"}"#);
                }
                CHANNEL
                    .lock()
                    .unwrap()
                    .0
                    .send(cmd_output.drain(..).collect())
                    .unwrap();
                DATA_ARRIVED.notify_all();
            }
        }
        (LogLevel::PRINT, _) => cmd_output.push_str(message),
        _ => (),
    }

    // TODO: allow to write to some log file instead
    // println!("--> {:?} {:?} {line} {dm_errno} {:?}", level, file, message);
}
