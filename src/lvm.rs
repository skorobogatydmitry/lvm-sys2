//! Safe wrapper for lvm2cmd.h bindings of the crate.  
//! It maintails a singletone [LVM] to run commands.  
//! The main interface is [Lvm::run], which runs the specified command and returns output as JSON or error if any.

use std::{
    collections::HashMap,
    ffi::{CStr, CString, NulError, c_char, c_int, c_void},
    str::FromStr,
    sync::{
        Condvar, LazyLock, Mutex,
        mpsc::{self, Receiver, Sender},
    },
};

pub use serde_json::Value;

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

/// LVM handle keeper
pub struct Lvm {
    handle: Box<c_void>,
}

// unsafe impl Sync for Lvm {}

impl Lvm {
    #[allow(rustdoc::private_intra_doc_links)]
    /// # Run LVM command using a global singleton [LVM]
    /// See `man 8 lvm` for list of available commands.  
    /// If you application is supposed to run as non-root, see [README.md / Non-root-execution](../index.html#non-root-execution)
    ///
    /// # Example
    /// ```
    /// use lvm_sys2::lvm::Lvm;
    /// let result = Lvm::run("lvs");
    /// assert!(result.is_ok());
    /// ```
    ///
    /// # Return value
    /// Ok variant contains a parsed JSON structure of what would bare `lvm <command> --reportformat json` give you  
    /// Err contains [CommandRetCode] - the reason why command execution failed. It could be:
    /// - recoverable (e.g. intermittent lvm2cmd errors)
    /// - permanent as in locks poisoning (e.g. log receiver panicked at some point)
    pub fn run(command: &str) -> Result<HashMap<String, serde_json::Value>, CommandRetCode> {
        Self::acquire_and(|lvm| lvm._run(format!("{command} {DEFAULT_LVM_FLAGS}")))
    }

    /// internal command runner
    fn _run(
        &mut self,
        command: String,
    ) -> Result<HashMap<String, serde_json::Value>, CommandRetCode> {
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
        let string_data = match ch.1.try_recv() {
            Ok(res) => res,
            Err(_) => match DATA_ARRIVED.wait(ch) {
                Ok(ch) => ch.1.recv().unwrap(), // UNWRAP: the other side cannot be closed - SENDER is static
                Err(_) => return Err(CommandRetCode::GlobalStatePoisoned),
            },
        };

        serde_json::from_str(&string_data)
            .map_err(|e| CommandRetCode::JsonDeserializationFailed((e, string_data)))
    }

    /// # Do NOT use, see [Lvm::run] instead
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
    pub fn acquire_and<Y, F: FnOnce(&mut Lvm) -> Result<Y, CommandRetCode>>(
        f: F,
    ) -> Result<Y, CommandRetCode> {
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
}

impl Drop for Lvm {
    fn drop(&mut self) {
        unsafe {
            lvm2_exit(self.handle.as_mut());
        }
    }
}

/// # Possible commands return codes
/// Contains both - native LVM codes and introduced by the wrapper
#[derive(Debug)]
pub enum CommandRetCode {
    // from lvm2cmd.h
    CommandSucceeded,
    NoSuchCommand,
    InvalidParameters,
    InitFailed,
    ProcessingFailed,
    // unknown (new) code returned by lvm2cmd.h
    Unknown(i32),

    // rust-specific "Codes"
    /// Command line contains \0 in the middle
    InvalidCommandLine(NulError),
    /// Global object is poisoned - some thread panic-ed on Lvm execution
    GlobalStatePoisoned,
    /// Channel to get data from logs is poisoned - some thread panic-ed on data send / receive
    DataChannelPoisoned,
    /// Serde error re-mapped - contains serde error and original string
    /// so the outer scope could still process it
    JsonDeserializationFailed((serde_json::Error, String)),
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

/// # Callback for LVM logs
/// It capture commands execution results by collecting all PRINT logs and passing it to the channel
/// ASSUMPTION: the underlying code guarantees that this fn gets called sequentially for sequential lines
///             JSON doc will be incorrect otherwise
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
